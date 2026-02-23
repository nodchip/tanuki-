#include "tanuki_progress.h"

#include <cstring>
#include <fstream>
#include <string>

#define INCBIN_SILENCE_BITCODE_WARNING
#include "incbin/incbin.h"

#include "misc.h"
#include "position.h"

using namespace YaneuraOu;

namespace {

constexpr char kProgressFilePath[] = "ProgressFilePath";
constexpr char kInternalPath[] = "<internal>";

YaneuraOu::OptionsMap* g_options = nullptr;
double g_weights[YaneuraOu::SQ_NB][YaneuraOu::Eval::fe_end] = {};

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
        if (gEmbeddedProgressSize != sizeof(g_weights)) {
            sync_cout << "info string Embedded progress size mismatch. expected=" << sizeof(g_weights)
                      << " actual=" << gEmbeddedProgressSize << sync_endl;
            return false;
        }

        std::memcpy(g_weights, gEmbeddedProgressData, sizeof(g_weights));
        sync_cout << "info string loading progress file : <internal>" << sync_endl;
        return true;
    }

    std::ifstream stream(file_path, std::ios::binary);
    if (!stream.is_open()) {
        sync_cout << "info string Failed to open the progress file. file_path=" << file_path << sync_endl;
        return false;
    }

    stream.read(reinterpret_cast<char*>(g_weights), sizeof(g_weights));
    if (!stream) {
        sync_cout << "info string Failed to read the progress file. file_path=" << file_path << sync_endl;
        return false;
    }

    sync_cout << "info string loading progress file : " << file_path << sync_endl;
    return true;
}

double Estimate(const YaneuraOu::Position& pos) {
    const auto sq_bk = pos.square<YaneuraOu::KING>(YaneuraOu::BLACK);
    const auto sq_wk = YaneuraOu::Inv(pos.square<YaneuraOu::KING>(YaneuraOu::WHITE));
    const auto& list0 = pos.eval_list()->piece_list_fb();
    const auto& list1 = pos.eval_list()->piece_list_fw();

    double sum = 0.0;
    for (int i = 0; i < YaneuraOu::PIECE_NUMBER_KING; ++i) {
        sum += g_weights[sq_bk][list0[i]];
        sum += g_weights[sq_wk][list1[i]];
    }

    return YaneuraOu::Math::sigmoid(sum);
}

}  // namespace Progress
}  // namespace Tanuki
