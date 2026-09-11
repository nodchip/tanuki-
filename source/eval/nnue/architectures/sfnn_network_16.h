// BulletOu nn16: byte activations, int16 FC weights, int64 accumulation.
#ifndef NNUE_SFNN_NETWORK_16_H_INCLUDED
#define NNUE_SFNN_NETWORK_16_H_INCLUDED

#include "../nnue_common.h"
#include <algorithm>
#include <cstdint>
#include <type_traits>

namespace YaneuraOu::Eval::NNUE {

template <IndexType Inputs, IndexType Outputs>
struct SfnnAffine16 {
    static constexpr IndexType PaddedInputs = CeilToMultiple<IndexType>(Inputs, 32);
    std::int32_t biases[Outputs];
    std::int16_t weights[Outputs][PaddedInputs];

    bool Read(std::istream& stream) {
        read_little_endian(stream, biases, Outputs);
        for (auto& row : weights)
            read_little_endian(stream, row, PaddedInputs);
        return bool(stream);
    }

    bool Write(std::ostream& stream) const {
        // Explicit byte order also supports hosts that are not little-endian.
        const auto write = [&](auto value) {
            using U = std::make_unsigned_t<decltype(value)>;
            const U bits = static_cast<U>(value);
            for (std::size_t i = 0; i < sizeof(value); ++i)
                stream.put(static_cast<char>((bits >> (i * 8)) & 255));
        };
        for (auto value : biases) write(value);
        for (const auto& row : weights)
            for (auto value : row) write(value);
        return bool(stream);
    }

    void Propagate(const std::uint8_t* input, std::int64_t* output) const {
        for (IndexType o = 0; o < Outputs; ++o) {
            std::int64_t sum = biases[o];
            for (IndexType i = 0; i < Inputs; ++i)
                sum += std::int64_t(weights[o][i]) * input[i];
            output[o] = sum;
        }
    }
};

template <IndexType InputDims, IndexType Hidden1Dims, IndexType Hidden2Dims, bool UseShortcut>
struct SfnnNetwork16 {
    static constexpr IndexType kInputDims = InputDims;
    static constexpr IndexType kHidden1Dims = Hidden1Dims;
    static constexpr IndexType kHidden2Dims = Hidden2Dims;
    static constexpr bool kUseShortcut = UseShortcut;
    static constexpr IndexType kOutputDimensions = 1;
    using OutputType = std::int64_t;
    SfnnAffine16<InputDims, Hidden1Dims + (UseShortcut ? 1 : 0)> fc_0;
    SfnnAffine16<Hidden1Dims * 2, Hidden2Dims> fc_1;
    SfnnAffine16<Hidden2Dims, 1> fc_2;
    std::uint32_t weight_scale = 4096;
    unsigned shift = 12;

    struct alignas(kCacheLineSize) Buffer {
        std::int64_t first[Hidden1Dims + (UseShortcut ? 1 : 0)];
        std::int64_t second[Hidden2Dims];
        std::uint8_t first_activated[Hidden1Dims * 2];
        std::uint8_t second_activated[Hidden2Dims];
        OutputType output[1];
    };
    static constexpr std::size_t kBufferSize = sizeof(Buffer);
    static constexpr std::uint32_t GetHashValue() { return 0x6333718Au; }

    bool SetWeightScale(std::uint32_t qb) {
        if (qb < 64 || qb > 16384 || (qb & (qb - 1))) return false;
        weight_scale = qb;
        shift = 0;
        while (qb >>= 1) ++shift;
        return true;
    }

    Tools::Result ReadParameters(std::istream& stream) {
        return fc_0.Read(stream) && fc_1.Read(stream) && fc_2.Read(stream)
            ? Tools::ResultCode::Ok : Tools::ResultCode::FileReadError;
    }
    bool WriteParameters(std::ostream& stream) const {
        return fc_0.Write(stream) && fc_1.Write(stream) && fc_2.Write(stream);
    }

    std::uint8_t Clipped(std::int64_t z) const {
        return std::uint8_t(std::clamp<std::int64_t>(z / weight_scale, 0, 127));
    }
    std::uint8_t Squared(std::int64_t z) const {
        // Beyond this bound the result saturates. Inside it, z*z fits int64
        // even for QB=16384. This also handles INT64_MIN without negation.
        const std::int64_t bound = std::int64_t(weight_scale) * 128;
        if (z <= -bound || z >= bound) return 127;
        return std::uint8_t(std::min<std::int64_t>((z * z) >> (2 * shift + 7), 127));
    }

    const OutputType* Propagate(const TransformedFeatureType* input, char* buffer) const {
        auto& b = *reinterpret_cast<Buffer*>(buffer);
        fc_0.Propagate(input, b.first);
        for (IndexType i = 0; i < Hidden1Dims; ++i) {
            b.first_activated[i] = Squared(b.first[i]);
            b.first_activated[Hidden1Dims + i] = Clipped(b.first[i]);
        }
        fc_1.Propagate(b.first_activated, b.second);
        for (IndexType i = 0; i < Hidden2Dims; ++i)
            b.second_activated[i] = Clipped(b.second[i]);
        fc_2.Propagate(b.second_activated, b.output);
        if constexpr (UseShortcut) b.output[0] += b.first[Hidden1Dims];
        // Restore the QB=64 units expected by FV_SCALE (truncate toward zero).
        b.output[0] /= weight_scale / 64;
        return b.output;
    }
};

} // namespace YaneuraOu::Eval::NNUE
#endif
