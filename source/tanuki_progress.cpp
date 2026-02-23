#include "tanuki_progress.h"

#include <algorithm>
#include <cassert>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <string>
#include <vector>

#define INCBIN_SILENCE_BITCODE_WARNING
#include "incbin/incbin.h"

#include "misc.h"
#include "position.h"

using namespace YaneuraOu;

namespace {

constexpr char kProgressFilePath[] = "ProgressFilePath";
constexpr char kInternalPath[] = "<internal>";

YaneuraOu::OptionsMap* g_options = nullptr;

#ifndef TANUKI_PROGRESS_NUMERIC_MODE
#define TANUKI_PROGRESS_NUMERIC_MODE 2
#endif

#ifndef TANUKI_PROGRESS_USE_DIFF
#define TANUKI_PROGRESS_USE_DIFF 0
#endif

#ifndef TANUKI_PROGRESS_TABLE_MODE
#define TANUKI_PROGRESS_TABLE_MODE 0
#endif

static_assert(TANUKI_PROGRESS_NUMERIC_MODE >= 0 && TANUKI_PROGRESS_NUMERIC_MODE <= 3,
              "TANUKI_PROGRESS_NUMERIC_MODE must be 0(double), 1(float), 2(Q0.15), or 3(Q16.16)");
static_assert(TANUKI_PROGRESS_USE_DIFF == 0 || TANUKI_PROGRESS_USE_DIFF == 1,
              "TANUKI_PROGRESS_USE_DIFF must be 0 or 1");
static_assert(TANUKI_PROGRESS_TABLE_MODE >= 0 && TANUKI_PROGRESS_TABLE_MODE <= 2,
              "TANUKI_PROGRESS_TABLE_MODE must be 0(none), 1(table), or 2(inline-binary)");

constexpr double kQ15Scale = 32768.0;
constexpr double kQ16Scale = 65536.0;

#if TANUKI_PROGRESS_NUMERIC_MODE == 0
using ProgressSum = double;
double g_weights[YaneuraOu::SQ_NB][YaneuraOu::Eval::fe_end] = {};
#elif TANUKI_PROGRESS_NUMERIC_MODE == 1
using ProgressSum = float;
float g_weights[YaneuraOu::SQ_NB][YaneuraOu::Eval::fe_end] = {};
#elif TANUKI_PROGRESS_NUMERIC_MODE == 2
using ProgressSum = int32_t;
int16_t g_weights_q15[YaneuraOu::SQ_NB][YaneuraOu::Eval::fe_end] = {};
#else
using ProgressSum = int64_t;
int32_t g_weights_q16[YaneuraOu::SQ_NB][YaneuraOu::Eval::fe_end] = {};
#endif

constexpr int kWeightCount = static_cast<int>(YaneuraOu::SQ_NB) * static_cast<int>(YaneuraOu::Eval::fe_end);
constexpr size_t kRawWeightsBytes = kWeightCount * sizeof(double);

int16_t to_q15(double value) {
    const double scaled = std::round(value * kQ15Scale);
    const double clamped = std::clamp(scaled, -32768.0, 32767.0);
    return static_cast<int16_t>(clamped);
}

int32_t to_q16(double value) {
    const double scaled = std::round(value * kQ16Scale);
    const double clamped = std::clamp(scaled, static_cast<double>(INT32_MIN), static_cast<double>(INT32_MAX));
    return static_cast<int32_t>(clamped);
}

void load_weights_from_raw(const double* raw_weights) {
    for (int sq = 0; sq < YaneuraOu::SQ_NB; ++sq) {
        for (int piece = 0; piece < YaneuraOu::Eval::fe_end; ++piece) {
            const int index = sq * static_cast<int>(YaneuraOu::Eval::fe_end) + piece;
#if TANUKI_PROGRESS_NUMERIC_MODE == 0
            g_weights[sq][piece] = raw_weights[index];
#elif TANUKI_PROGRESS_NUMERIC_MODE == 1
            g_weights[sq][piece] = static_cast<float>(raw_weights[index]);
#elif TANUKI_PROGRESS_NUMERIC_MODE == 2
            g_weights_q15[sq][piece] = to_q15(raw_weights[index]);
#else
            g_weights_q16[sq][piece] = to_q16(raw_weights[index]);
#endif
        }
    }
}

inline bool is_valid_bona_piece(int value) {
    return value >= 0 && value < static_cast<int>(YaneuraOu::Eval::fe_end);
}

inline ProgressSum contribution(YaneuraOu::Square sq, int bona_piece) {
    if (!is_valid_bona_piece(bona_piece)) return static_cast<ProgressSum>(0);
#if TANUKI_PROGRESS_NUMERIC_MODE == 0 || TANUKI_PROGRESS_NUMERIC_MODE == 1
    return g_weights[sq][bona_piece];
#elif TANUKI_PROGRESS_NUMERIC_MODE == 2
    return static_cast<ProgressSum>(g_weights_q15[sq][bona_piece]);
#else
    return static_cast<ProgressSum>(g_weights_q16[sq][bona_piece]);
#endif
}

inline double sum_to_double(ProgressSum sum) {
#if TANUKI_PROGRESS_NUMERIC_MODE == 2
    return static_cast<double>(sum) / kQ15Scale;
#elif TANUKI_PROGRESS_NUMERIC_MODE == 3
    return static_cast<double>(sum) / kQ16Scale;
#else
    return static_cast<double>(sum);
#endif
}

inline ProgressSum cache_to_sum(double value) {
#if TANUKI_PROGRESS_NUMERIC_MODE == 1
    return static_cast<float>(value);
#elif TANUKI_PROGRESS_NUMERIC_MODE == 3
    return static_cast<int64_t>(std::llround(value));
#else
    return static_cast<ProgressSum>(value);
#endif
}

inline double sum_to_cache(ProgressSum sum) {
    return static_cast<double>(sum);
}

#if TANUKI_PROGRESS_NUMERIC_MODE == 0
inline int table_index_linear(ProgressSum sum) {
    static constexpr double kThresholds[7] = {
        -1.9459101490553132, -1.0986122886681098, -0.5108256237659907, 0.0,
        0.5108256237659907, 1.0986122886681098, 1.9459101490553132,
    };
    int idx = 0;
    while (idx < 7 && sum >= kThresholds[idx]) ++idx;
    return idx;
}

inline int table_index_inline_binary(ProgressSum sum) {
    if (sum < 0.0) {
        if (sum < -1.0986122886681098) return (sum < -1.9459101490553132) ? 0 : 1;
        return (sum < -0.5108256237659907) ? 2 : 3;
    }
    if (sum < 0.5108256237659907) return 4;
    if (sum < 1.0986122886681098) return 5;
    return (sum < 1.9459101490553132) ? 6 : 7;
}
#elif TANUKI_PROGRESS_NUMERIC_MODE == 1
inline int table_index_linear(ProgressSum sum) {
    static constexpr float kThresholds[7] = {
        -1.9459101f, -1.0986123f, -0.51082563f, 0.0f, 0.51082563f, 1.0986123f, 1.9459101f,
    };
    int idx = 0;
    while (idx < 7 && sum >= kThresholds[idx]) ++idx;
    return idx;
}

inline int table_index_inline_binary(ProgressSum sum) {
    if (sum < 0.0f) {
        if (sum < -1.0986123f) return (sum < -1.9459101f) ? 0 : 1;
        return (sum < -0.51082563f) ? 2 : 3;
    }
    if (sum < 0.51082563f) return 4;
    if (sum < 1.0986123f) return 5;
    return (sum < 1.9459101f) ? 6 : 7;
}
#elif TANUKI_PROGRESS_NUMERIC_MODE == 2
inline int table_index_linear(ProgressSum sum_q15) {
    static constexpr int32_t kThresholdsQ15[7] = {
        -63764, -35999, -16739, 0, 16739, 35999, 63764,
    };
    int idx = 0;
    while (idx < 7 && sum_q15 >= kThresholdsQ15[idx]) ++idx;
    return idx;
}

inline int table_index_inline_binary(ProgressSum sum_q15) {
    if (sum_q15 < 0) {
        if (sum_q15 < -35999) return (sum_q15 < -63764) ? 0 : 1;
        return (sum_q15 < -16739) ? 2 : 3;
    }
    if (sum_q15 < 16739) return 4;
    if (sum_q15 < 35999) return 5;
    return (sum_q15 < 63764) ? 6 : 7;
}
#else
inline int table_index_linear(ProgressSum sum_q16) {
    static constexpr int32_t kThresholdsQ16[7] = {
        -127527, -71999, -33477, 0, 33477, 71999, 127527,
    };
    int idx = 0;
    while (idx < 7 && sum_q16 >= kThresholdsQ16[idx]) ++idx;
    return idx;
}

inline int table_index_inline_binary(ProgressSum sum_q16) {
    if (sum_q16 < 0) {
        if (sum_q16 < -71999) return (sum_q16 < -127527) ? 0 : 1;
        return (sum_q16 < -33477) ? 2 : 3;
    }
    if (sum_q16 < 33477) return 4;
    if (sum_q16 < 71999) return 5;
    return (sum_q16 < 127527) ? 6 : 7;
}
#endif

ProgressSum compute_full_sum(const YaneuraOu::Position& pos, YaneuraOu::Square sq_bk, YaneuraOu::Square sq_wk) {
    const auto& list0 = pos.eval_list()->piece_list_fb();
    const auto& list1 = pos.eval_list()->piece_list_fw();

    ProgressSum sum = 0;
    for (int i = 0; i < YaneuraOu::PIECE_NUMBER_KING; ++i) {
        sum += contribution(sq_bk, list0[i]);
        sum += contribution(sq_wk, list1[i]);
    }
    return sum;
}

bool try_get_sum_with_cache_or_diff(const YaneuraOu::Position& pos, YaneuraOu::Square sq_bk, YaneuraOu::Square sq_wk,
                                    ProgressSum& sum) {
#if TANUKI_PROGRESS_USE_DIFF == 1
    auto* st = pos.state();
    if (st->tanuki_progress_valid && st->tanuki_progress_key == pos.key() && st->tanuki_progress_sq_bk == sq_bk &&
        st->tanuki_progress_sq_wk == sq_wk) {
        sum = cache_to_sum(st->tanuki_progress_sum);
        return true;
    }
#else
    (void)pos;
    (void)sq_bk;
    (void)sq_wk;
    (void)sum;
#endif
    return false;
}

void store_sum_cache(const YaneuraOu::Position& pos, YaneuraOu::Square sq_bk, YaneuraOu::Square sq_wk, ProgressSum sum) {
    auto* st = pos.state();
    st->tanuki_progress_key = pos.key();
    st->tanuki_progress_sum = sum_to_cache(sum);
    st->tanuki_progress_sq_bk = sq_bk;
    st->tanuki_progress_sq_wk = sq_wk;
    st->tanuki_progress_valid = true;
}

int baseline_stack_index_from_sigmoid(ProgressSum sum) {
    int idx = static_cast<int>(YaneuraOu::Math::sigmoid(sum_to_double(sum)) * 8.0);
    if (idx < 0) idx = 0;
    if (idx > 7) idx = 7;
    return idx;
}

#if !defined(_MSC_VER)
INCBIN(EmbeddedProgress, "progress.bin");
#else
const unsigned char gEmbeddedProgressData[1] = {0};
const unsigned char* const gEmbeddedProgressEnd = &gEmbeddedProgressData[1];
const unsigned int gEmbeddedProgressSize = 1;
#endif

}  // namespace

namespace Tanuki {
namespace Progress {

bool add_options(YaneuraOu::OptionsMap& options) {
    g_options = &options;
    options.add(kProgressFilePath, YaneuraOu::Option(kInternalPath));
    return true;
}

bool Load() {
    if (g_options == nullptr) {
        sync_cout << "info string Progress options are not initialized." << sync_endl;
        return false;
    }

    const std::string file_path = (*g_options)[kProgressFilePath];

    if (file_path == kInternalPath) {
        if (gEmbeddedProgressSize != kRawWeightsBytes) {
            sync_cout << "info string Embedded progress size mismatch. expected=" << kRawWeightsBytes
                      << " actual=" << gEmbeddedProgressSize << sync_endl;
            return false;
        }

        std::vector<double> raw_weights(kWeightCount);
        std::memcpy(raw_weights.data(), gEmbeddedProgressData, kRawWeightsBytes);
        load_weights_from_raw(raw_weights.data());
        sync_cout << "info string loading progress file : <internal>" << sync_endl;
        return true;
    }

    std::ifstream stream(file_path, std::ios::binary);
    if (!stream.is_open()) {
        sync_cout << "info string Failed to open the progress file. file_path=" << file_path << sync_endl;
        return false;
    }

    std::vector<double> raw_weights(kWeightCount);
    stream.read(reinterpret_cast<char*>(raw_weights.data()), kRawWeightsBytes);
    if (!stream) {
        sync_cout << "info string Failed to read the progress file. file_path=" << file_path << sync_endl;
        return false;
    }
    load_weights_from_raw(raw_weights.data());

    sync_cout << "info string loading progress file : " << file_path << sync_endl;
    return true;
}

double Estimate(const YaneuraOu::Position& pos) {
    const auto sq_bk = pos.square<YaneuraOu::KING>(YaneuraOu::BLACK);
    const auto sq_wk = YaneuraOu::Inv(pos.square<YaneuraOu::KING>(YaneuraOu::WHITE));

    ProgressSum sum = 0;
    if (!try_get_sum_with_cache_or_diff(pos, sq_bk, sq_wk, sum)) {
        sum = compute_full_sum(pos, sq_bk, sq_wk);
    }

#if TANUKI_PROGRESS_USE_DIFF == 1
    // 差分計算の結果がフル計算と一致することをデバッグ時に保証する。
    const ProgressSum full_sum = compute_full_sum(pos, sq_bk, sq_wk);
    assert(sum == full_sum);
    store_sum_cache(pos, sq_bk, sq_wk, sum);
#endif

#if TANUKI_PROGRESS_TABLE_MODE == 0
    return YaneuraOu::Math::sigmoid(sum_to_double(sum));
#elif TANUKI_PROGRESS_TABLE_MODE == 1
    const int idx = table_index_linear(sum);
    assert(idx == baseline_stack_index_from_sigmoid(sum));
    return static_cast<double>(idx) * (1.0 / 8.0);
#else
    const int idx = table_index_inline_binary(sum);
    assert(idx == baseline_stack_index_from_sigmoid(sum));
    return static_cast<double>(idx) * (1.0 / 8.0);
#endif
}

}  // namespace Progress
}  // namespace Tanuki
