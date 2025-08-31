#include "tanuki_kifu_shuffler.h"
#include "config.h"

#ifdef EVAL_LEARN

#include <cstdio>
#include <ctime>
#include <direct.h>
#include <filesystem>
#include <omp.h>
#include <random>

#include "learn/learn.h"
#include "misc.h"
#include "tanuki_kifu_reader.h"
#include "tanuki_kifu_writer.h"
#include "tanuki_progress.h"
#include "thread.h"

using Learner::PackedSfenValue;

namespace {
    static const constexpr char* kShuffledKifuDir = "ShuffledKifuDir";
    static const constexpr char* kShuffledMinPly = "ShuffledMinPly";
    static const constexpr char* kShuffledMaxPly = "ShuffledMaxPly";
    static const constexpr char* kShuffledMinProgress = "ShuffledMinProgress";
    static const constexpr char* kShuffledMaxProgress = "ShuffledMaxProgress";
    static const constexpr char* kApplyQSearch = "ApplyQSearch";
    // シャッフル後のファイル数
    // Windowsでは一度に512個までのファイルしか開けないため
    // 256個に制限しておく
    static const constexpr int kNumShuffledKifuFiles = 256;
    static const constexpr int kMaxPackedSfenValues = 1024 * 1024;
}

void Tanuki::InitializeShuffler(USI::OptionsMap& o) {
    o[kShuffledKifuDir] << USI::Option("kifu_shuffled");
    o[kShuffledMinPly] << USI::Option(1, 1, std::numeric_limits<u64>::max() / 2);
    o[kShuffledMaxPly] << USI::Option(std::numeric_limits<u64>::max() / 2, 1, std::numeric_limits<u64>::max() / 2);
    o[kShuffledMinProgress] << USI::Option("0.0");
    o[kShuffledMaxProgress] << USI::Option("1.0");
    o[kApplyQSearch] << USI::Option(false);
}

void Tanuki::ShuffleKifu(Position& position) {
    GlobalOptions_ old_global_options = GlobalOptions;
    GlobalOptions.use_eval_hash = false;
    GlobalOptions.use_hash_probe = false;

    int num_threads = (int)Options["Threads"];
    omp_set_num_threads(num_threads);

    Search::LimitsType limits;
    // 引き分けの手数付近で引き分けの値が返るのを防ぐため1 << 16にする
    limits.max_game_ply = 1 << 16;
    limits.depth = MAX_PLY;
    limits.silent = true;
    limits.enteringKingRule = EKR_27_POINT;
    Search::Limits = limits;

    // 棋譜を入力し、複数のファイルにランダムに追加していく
    sync_cout << "info string Reading and dividing kifu files..." << sync_endl;

    std::string kifu_dir = Options["KifuDir"];
    std::string shuffled_kifu_dir = Options[kShuffledKifuDir];
    u64 min_ply = Options[kShuffledMinPly];
    u64 max_ply = Options[kShuffledMaxPly];
    double min_progress = std::atof(static_cast<std::string>(Options[kShuffledMinProgress]).c_str());
    double max_progress = std::atof(static_cast<std::string>(Options[kShuffledMaxProgress]).c_str());
    bool apply_qsearch = Options[kApplyQSearch];

    sync_cout << "kifu_dir=" << kifu_dir << sync_endl;
    sync_cout << "shuffled_kifu_dir=" << shuffled_kifu_dir << sync_endl;

    auto reader = std::make_unique<KifuReader>(kifu_dir, 1);
    _mkdir(shuffled_kifu_dir.c_str());

    std::vector<std::string> file_paths;
    for (int file_index = 0; file_index < kNumShuffledKifuFiles; ++file_index) {
        char file_path[_MAX_PATH];
        sprintf(file_path, "%s/shuffled.%03d.bin", shuffled_kifu_dir.c_str(), file_index);
        file_paths.push_back(file_path);
    }

    sync_cout << "info string Opening output files..." << sync_endl;
    std::vector<std::shared_ptr<KifuWriter> > writers;
    for (const auto& file_path : file_paths) {
        //sync_cout << "file_path=" << file_path << sync_endl;
        writers.push_back(std::make_shared<KifuWriter>(file_path));
    }

    sync_cout << "info string Starting dividing..." << sync_endl;

    std::mt19937_64 mt(std::time(nullptr));
    std::uniform_int_distribution<> dist(0, kNumShuffledKifuFiles - 1);
    int64_t num_records = 0;
    u64 current_ply = 1;

    Tanuki::Progress progress_estimator;
    if (!progress_estimator.Load()) {
        sync_cout << "info string Failed to load the progress file..." << sync_endl;
        std::exit(1);
    }

    for (;;) {
        std::vector<PackedSfenValue> records;
        {
            PackedSfenValue record;
            while (static_cast<int>(records.size()) < kMaxPackedSfenValues && reader->Read(record)) {
                StateInfo state_info = {};
                Tools::Result result = position.set_from_packed_sfen(record.sfen, &state_info, Threads[0]);
                if (result.is_not_ok()) {
                    sync_cout << "Failed to call set_from_packed_sfen()." << std::endl << position << sync_endl;
                    continue;
                }

                double progress = 0.0;
                if (min_progress != 0.0 || max_progress != 1.0) {
                    // 高速化のため、min_progressまたはmax_progressが設定されていた場合のみ
                    // 進行度を推定する。
                    progress = progress_estimator.Estimate(position);
                }

                if (min_ply <= current_ply && current_ply <= max_ply &&
                    min_progress <= progress && progress <= max_progress) {
                    records.push_back(record);
                }

                // 最後の局面を読み込んだら、手数をリセットする。
                if (record.last_position) {
                    current_ply = 1;
                }
                else {
                    ++current_ply;
                }
            }
        }

        if (records.empty()) {
            break;
        }

        if (apply_qsearch) {
            std::atomic<int> global_record_index;
            global_record_index = 0;

#pragma omp parallel
            {
                int thread_index = ::omp_get_thread_num();
                WinProcGroup::bindThisThread(thread_index);
                std::vector<StateInfo> state_info(MAX_PLY);
                Thread& thread = *Threads[thread_index];
                Position& pos = thread.rootPos;
                for (int local_record_index = global_record_index++;
                    local_record_index < static_cast<int>(records.size());
                    local_record_index = global_record_index++) {
                    PackedSfenValue& record = records[local_record_index];
                    if (pos.set_from_packed_sfen(record.sfen, &state_info[0], &thread).code != Tools::Result::Ok().code) {
                        sync_cout << "Failed to call set_from_packed_sfen()." << std::endl << pos << sync_endl;
                        std::exit(1);
                    }

                    auto root_color = pos.side_to_move();

                    if (pos.this_thread() == nullptr) {
                        sync_cout << "pos.this_thread() == nullptr." << sync_endl;
                        std::exit(1);
                    }

                    auto value_and_pv = Learner::qsearch(pos);

                    int ply = 1;
                    for (auto m : value_and_pv.second)
                    {
                        ASSERT_LV3(pos.pseudo_legal(m) && pos.legal(m));
                        pos.do_move(m, state_info[ply++]);
                    }

                    pos.sfen_pack(record.sfen);

                    auto leaf_color = pos.side_to_move();

                    record.move = MOVE_NONE;

                    // Root局面と末端局面の手番が異なる場合、評価値と勝敗を反転する。
                    if (root_color != leaf_color) {
                        record.score = -record.score;
                        record.game_result = -record.game_result;
                    }
                }
            }
        }

        for (const auto& record : records) {
            if (!writers[dist(mt)]->Write(record)) {
                sync_cout << "info string Failed to write a record to a kifu file. " << sync_endl;
            }
            ++num_records;
            if (num_records % 10000000 == 0) {
                sync_cout << "info string " << num_records << sync_endl;
            }
        }
    }
    for (auto& writer : writers) {
        writer->Close();
    }

    sync_cout << "info string Starting shuffling..." << sync_endl;

    // 出力ファイルを準備する。
    FILE* output_file = nullptr;
    {
        char file_path[_MAX_PATH];
        sprintf(file_path, "%s/shuffled.bin", shuffled_kifu_dir.c_str());
        output_file = std::fopen(file_path, "wb");

        if (std::setvbuf(output_file, nullptr, _IOFBF, std::numeric_limits<int>::max())) {
            sync_cout << "info string Failed to set the output buffer: output_file_path_="
                << file_path << sync_endl;
            return;
        }
    }

    // 各ファイルをシャッフルする
    for (const auto& file_path : file_paths) {
        sync_cout << "info string " << file_path << sync_endl;

        // ファイル全体を読み込む
        FILE* input_file = std::fopen(file_path.c_str(), "rb");
        if (input_file == nullptr) {
            sync_cout << "info string Failed to open a kifu file. " << file_path << sync_endl;
            return;
        }

        if (std::setvbuf(input_file, nullptr, _IOFBF, std::numeric_limits<int>::max())) {
            sync_cout << "info string Failed to set the output buffer: input_file="
                << file_path << sync_endl;
            return;
        }

        _fseeki64(input_file, 0, SEEK_END);
        int64_t size = _ftelli64(input_file);
        _fseeki64(input_file, 0, SEEK_SET);
        std::vector<PackedSfenValue> records(size / sizeof(PackedSfenValue));
        std::fread(&records[0], sizeof(PackedSfenValue), size / sizeof(PackedSfenValue), input_file);
        std::fclose(input_file);
        input_file = nullptr;

        // 棋譜全体をシャッフルする
        std::shuffle(records.begin(), records.end(), mt);

        // シャッフル済みファイルを削除する
        std::filesystem::remove(file_path);

        // 出力ファイルに書き出す
        if (std::fwrite(&records[0], sizeof(PackedSfenValue), records.size(), output_file) !=
            records.size()) {
            sync_cout << "info string Failed to write records to a kifu file. " << file_path
                << sync_endl;
            return;
        }
    }

    std::fclose(output_file);
    output_file = nullptr;

    GlobalOptions = old_global_options;
}

#endif
