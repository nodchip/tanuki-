#include "tanuki_kifu_generator.h"
#include "config.h"

#ifdef EVAL_LEARN

#include <omp.h>
#include <atomic>
#include <cmath>
#include <cstdlib>
#include <ctime>
#include <filesystem>
#include <fstream>
#include <memory>
#include <random>
#include <sstream>

#include "learn/learn.h"
#include "misc.h"
#include "search.h"
#include "tanuki_kifu_writer.h"
#include "tanuki_progress_report.h"
#include "tanuki_s_book_black_start_position_picker.h"
#include "tanuki_sfen_start_position_picker.h"
#include "thread.h"

#ifdef abs
#undef abs
#endif

using Search::RootMove;
using USI::Option;
using USI::OptionsMap;

namespace {
	constexpr int kMaxGamePlay = 400;
	constexpr int kMaxSwapTrials = 10;
	constexpr int kMaxTrialsToSelectSquares = 100;

	enum GameResult {
		GameResultWin = 1,
		GameResultLose = -1,
		GameResultDraw = 0,
	};

	constexpr const char* kOptionKifuDir = "KifuDir";
	constexpr const char* kOptionGeneratorNumPositions = "GeneratorNumPositions";
	constexpr const char* kOptionGeneratorSearchDepth = "GeneratorSearchDepth";
	constexpr const char* kOptionGeneratorKifuTag = "GeneratorKifuTag";
	constexpr const char* kOptionGeneratorStartposFileName = "GeneratorStartposFileName";
	constexpr const char* kOptionGeneratorValueThreshold = "GeneratorValueThreshold";
	constexpr const char* kOptionGeneratorOptimumNodesSearched = "GeneratorOptimumNodesSearched";
	constexpr const char* kOptionGeneratorMeasureDepth = "GeneratorMeasureDepth";
	constexpr const char* kOptionGeneratorStartPositionMaxPlay = "GeneratorStartPositionMaxPlay";
	constexpr const char* kOptionGeneratorRandomMultiPV = "GeneratorRandomMultiPV";
	constexpr const char* kOptionGeneratorMinMultiPVPlay = "GeneratorMinMultiPVPlay";
	constexpr const char* kOptionGeneratorMaxMultiPVPlay = "GeneratorMaxMultiPVPlay";
	constexpr const char* kOptionGeneratorMaxMultiPVMoves = "GeneratorMaxMultiPVMoves";
	constexpr const char* kOptionGeneratorMaxEvalDiff = "GeneratorMaxEvalDiff";
	constexpr const char* kOptionGeneratorRandomMove = "GeneratorRandomMove";
	constexpr const char* kOptionGeneratorStartposType = "GeneratorStartposType";
	constexpr const char* kOptionConvertSfenToLearningDataInputSfenFileName =
		"ConvertSfenToLearningDataInputSfenFileName";
	constexpr const char* kOptionConvertSfenToLearningDataSearchDepth =
		"ConvertSfenToLearningDataSearchDepth";
	constexpr const char* kOptionConvertSfenToLearningDataOutputFileName =
		"ConvertSfenToLearningDataOutputFileName";
	constexpr const char* kValueSfen = "sfen";
	constexpr const char* kValueSBookBlack = "s-book_black";

	template <typename T>
	T ParseOptionOrDie(const char* name) {
		std::string value_string = (std::string)Options[name];
		std::istringstream iss(value_string);
		T value;
		if (!(iss >> value)) {
			sync_cout << "Failed to parse an option. Exitting...: name=" << name << " value=" << value
				<< sync_endl;
			std::exit(1);
		}
		return value;
	}
}

void Tanuki::InitializeGenerator(USI::OptionsMap& o) {
	o[kOptionKifuDir] << Option("");
	o[kOptionGeneratorNumPositions] << Option("10000000000");
	o[kOptionGeneratorSearchDepth] << Option(8, 1, MAX_PLY);
	o[kOptionGeneratorKifuTag] << Option("default_tag");
	o[kOptionGeneratorStartposFileName] << Option("startpos.sfen");
	o[kOptionGeneratorStartposType] << Option(std::vector<std::string>({ kValueSfen, kValueSBookBlack }), kValueSfen);
	o[kOptionGeneratorValueThreshold] << Option(VALUE_MATE, 0, VALUE_MATE);
	o[kOptionGeneratorOptimumNodesSearched] << Option("0");
	o[kOptionGeneratorMeasureDepth] << Option(false);
	o[kOptionGeneratorStartPositionMaxPlay] << Option(std::numeric_limits<int>::max(), 1, std::numeric_limits<int>::max());
	o[kOptionConvertSfenToLearningDataInputSfenFileName] << Option("nyugyoku_win.sfen");
	o[kOptionConvertSfenToLearningDataSearchDepth] << Option(12, 1, MAX_PLY);
	o[kOptionConvertSfenToLearningDataOutputFileName] << Option("nyugyoku_win.bin");
	o[kOptionGeneratorRandomMultiPV] << Option(1, 1, std::numeric_limits<int>::max());
	o[kOptionGeneratorMinMultiPVPlay] << Option(1, 1, std::numeric_limits<int>::max());
	o[kOptionGeneratorMaxMultiPVPlay] << Option(32, 1, std::numeric_limits<int>::max());
	o[kOptionGeneratorMaxMultiPVMoves] << Option(16, 0, std::numeric_limits<int>::max());
	o[kOptionGeneratorMaxEvalDiff] << Option(30, 0, std::numeric_limits<int>::max());
	o[kOptionGeneratorRandomMove] << Option(true);
}

namespace {
	void RandomMove(Position& pos, StateInfo* state, std::mt19937_64& mt) {
		ASSERT_LV3(pos.this_thread());
		auto& root_moves = pos.this_thread()->rootMoves;

		root_moves.clear();
		for (auto m : MoveList<LEGAL>(pos)) {
			root_moves.push_back(Search::RootMove(m));
		}

		// 50%の確率で玉の移動のみを行う
		if (mt() & 1) {
			root_moves.erase(std::remove_if(root_moves.begin(), root_moves.end(),
				[&pos](const auto& root_move) {
					Move move = root_move.pv.front();
					if (is_drop(move)) {
						return true;
					}
					Square sq = from_sq(move);
					return pos.king_square(BLACK) != sq &&
						pos.king_square(WHITE) != sq;
				}),
				root_moves.end());
		}

		if (root_moves.empty()) {
			return;
		}

		std::uniform_int_distribution<> dist(0, static_cast<int>(root_moves.size()) - 1);
		pos.do_move(root_moves[dist(mt)].pv[0], state[pos.game_ply()]);
		Eval::evaluate(pos);
	}
}

void Tanuki::GenerateKifu() {
	std::srand(static_cast<unsigned int>(std::time(nullptr)));

	// 開始局面集を読み込む。
	std::unique_ptr<StartPositionPicker> start_position_picker;
	auto startpos_type = Options[kOptionGeneratorStartposType];
	if (startpos_type == kValueSfen) {
		start_position_picker.reset(new SfenStartPositionPicker());
	}
	else if (startpos_type == kValueSBookBlack) {
		start_position_picker.reset(new SBookBlackStartPositionPicker());
	}
	else {
		sync_cout << "info string Invalid startpos_type. startpos_type=" << startpos_type << sync_endl;
		return;
	}

	if (!start_position_picker->Open()) {
		return;
	}

	int num_threads = (int)Options["Threads"];
	omp_set_num_threads(num_threads);

	// ここでEval::load_eval()を呼ぶと、Large Pageを使用している場合にメモリの確保に失敗し、クラッシュする。
	//Eval::load_eval();

	std::string kifu_directory = (std::string)Options["KifuDir"];
	std::filesystem::create_directories(kifu_directory);

	int search_depth = Options[kOptionGeneratorSearchDepth];
	int64_t num_positions = ParseOptionOrDie<int64_t>(kOptionGeneratorNumPositions);
	int value_threshold = Options[kOptionGeneratorValueThreshold];
	std::string output_file_name_tag = Options[kOptionGeneratorKifuTag];
	uint64_t optimum_nodes_searched =
		ParseOptionOrDie<uint64_t>(kOptionGeneratorOptimumNodesSearched);
	bool measure_depth = Options[kOptionGeneratorMeasureDepth];
	int random_multi_pv = Options[kOptionGeneratorRandomMultiPV];
	int min_multi_pv_play = Options[kOptionGeneratorMinMultiPVPlay];
	int max_multi_pv_play = Options[kOptionGeneratorMaxMultiPVPlay];
	int max_multi_pv_moves = Options[kOptionGeneratorMaxMultiPVMoves];
	int max_eval_diff = Options[kOptionGeneratorMaxEvalDiff];
	bool random_move = Options[kOptionGeneratorRandomMove];

	std::cout << "search_depth=" << search_depth << std::endl;
	std::cout << "num_positions=" << num_positions << std::endl;
	std::cout << "value_threshold=" << value_threshold << std::endl;
	std::cout << "output_file_name_tag=" << output_file_name_tag << std::endl;
	std::cout << "optimum_nodes_searched=" << optimum_nodes_searched << std::endl;
	std::cout << "measure_depth=" << measure_depth << std::endl;
	std::cout << "random_multi_pv=" << random_multi_pv << std::endl;
	std::cout << "min_multi_pv_play=" << min_multi_pv_play << std::endl;
	std::cout << "max_multi_pv_play=" << max_multi_pv_play << std::endl;
	std::cout << "max_multi_pv_count=" << max_multi_pv_moves << std::endl;
	std::cout << "max_eval_diff=" << max_eval_diff << std::endl;
	std::cout << "random_move=" << random_move << std::endl;

	Search::LimitsType limits;
	// 引き分けの手数付近で引き分けの値が返るのを防ぐため1 << 16にする
	limits.max_game_ply = 1 << 16;
	limits.depth = MAX_PLY;
	limits.silent = true;
	limits.enteringKingRule = EKR_27_POINT;
	Search::Limits = limits;

	// 何手目 -> 探索深さ
	std::vector<std::vector<int> > game_play_to_depths(kMaxGamePlay + 1);

	time_t start_time;
	std::time(&start_time);

	// スレッド間で共有する
	std::atomic_int64_t global_position_index;
	global_position_index = 0;
	ProgressReport progress_report(num_positions, 60 * 60);
	std::mutex mutex_game_play_to_depths;
	std::atomic<bool> need_wait = false;
	std::atomic<int> num_records = 0;

#pragma omp parallel
	{
		int thread_index = ::omp_get_thread_num();
		WinProcGroup::bindThisThread(thread_index);
		char output_file_path[1024];
		std::sprintf(output_file_path,
			"%s/kifu.tag=%s.depth=%d.num_positions=%I64d.start_time=%I64d.thread_index=%03d.bin",
			kifu_directory.c_str(), output_file_name_tag.c_str(), search_depth, num_positions,
			start_time, thread_index);
		// 各スレッドに持たせる
		std::unique_ptr<KifuWriter> kifu_writer =
			std::make_unique<KifuWriter>(output_file_path);
		std::mt19937_64 mt19937_64(start_time + thread_index);

		std::vector<StateInfo> state_info(1024);
		while (global_position_index < num_positions) {
			++num_records;
			Thread& thread = *Threads[thread_index];
			Position& pos = thread.rootPos;
			StateInfo* state_info_ptr = &state_info[0];
			start_position_picker->Pick(pos, state_info_ptr, thread);

			// 自分スレッド用の置換表があるはずなので自分の置換表だけをクリアする。
			//thread.tt.clear();

			if (random_move) {
				RandomMove(pos, state_info_ptr++, mt19937_64);
			}

			std::vector<Learner::PackedSfenValue> records;
			int num_multi_pv_move = 0;
			while (
				// 一定の手数に達していない
				pos.game_ply() < kMaxGamePlay &&
				// 詰まされていない
				!pos.is_mated() &&
				// 宣言勝ちができない
				pos.DeclarationWin() == MOVE_NONE &&
				// 千日手による引き分けではない
				// 優等局面・劣等局面は、対局中に一瞬だけ現れ、その後通常通り対局が進むパターンがあるため、考慮しない
				pos.is_repetition() != RepetitionState::REPETITION_DRAW &&
				// 評価値の絶対値が一定値以内
				(records.empty() || std::abs(records.back().score) < value_threshold)) {

				int multi_pv = 1;
				if (min_multi_pv_play <= pos.game_ply() &&
					pos.game_ply() <= max_multi_pv_play &&
					num_multi_pv_move < max_multi_pv_moves &&
					std::uniform_int_distribution<int>(0, 1)(mt19937_64) == 1) {
					multi_pv = random_multi_pv;
					++num_multi_pv_move;
				}

				Learner::search(pos, search_depth, multi_pv, optimum_nodes_searched);

				const auto& root_moves = pos.this_thread()->rootMoves;
				multi_pv = std::min<int>(multi_pv, root_moves.size());

				int num_valid_moves = 0;
				for (int pv_index = 0; pv_index < multi_pv; ++pv_index) {
					if (root_moves[0].score - max_eval_diff <= root_moves[pv_index].score) {
						num_valid_moves = pv_index + 1;
					}
				}

				int selected_move_index = std::uniform_int_distribution<int>(
					0, num_valid_moves - 1)(mt19937_64);
				const auto& root_move = root_moves[selected_move_index];
				// 選ばれた指し手のスコアをこの局面のスコアとして記録する
				const std::vector<Move>& pv = root_move.pv;

				// 詰みの場合はpvが空になる
				if (pv.empty()) {
					break;
				}

				// 局面が不正な場合があるので再度チェックする
				if (!pos.pos_is_ok()) {
					break;
				}

				Move pv_move = pv[0];

				Learner::PackedSfenValue record = {};
				pos.sfen_pack(record.sfen);
				record.score = root_move.score;
				record.gamePly = pos.game_ply();
				record.move = pv_move;
				records.push_back(record);

				pos.do_move(pv_move, *state_info_ptr++);
				// 差分計算のためevaluate()を呼び出す
				Eval::evaluate(pos);

				// 手数毎の探索の深さを記録しておく
				// 何らかの形で勝ちが決まっている局面は
				// 探索深さが極端に深くなるため除外する
				if (measure_depth && abs(root_move.score) < VALUE_KNOWN_WIN) {
					std::lock_guard<std::mutex> lock(mutex_game_play_to_depths);
					game_play_to_depths[pos.game_ply()].push_back(thread.rootDepth);
				}
			}

			int game_result = GameResultDraw;
			u8 entering_king = 0;
			if (pos.is_mated()) {
				// 負け
				// 詰まされた
				// records.back()は相手局面なので勝ち
				game_result = GameResultWin;
			}
			else if (pos.DeclarationWin() != MOVE_NONE) {
				// 勝ち
				// 入玉勝利
				// records.back()は相手局面なので負け
				game_result = GameResultLose;
				entering_king = 1;
			}
			else if (!records.empty() && records.back().score > value_threshold) {
				// 勝ち
				// records.back()は自分の局面なので勝ち
				game_result = GameResultWin;
			}
			else if (!records.empty() && records.back().score < -value_threshold) {
				// 負け
				// records.back()は自分の局面なので負け
				game_result = GameResultLose;
			}
			else {
				// 引き分け、一定手数に到達した場合は
				// 学習データに含めない。
				continue;
			}

			for (auto rit = records.rbegin(); rit != records.rend(); ++rit) {
				rit->game_result = game_result;
				rit->entering_king = entering_king;
				game_result = -game_result;
			}

			if (!records.empty()) {
				records.back().last_position = true;
			}

			for (const auto& record : records) {
				if (!kifu_writer->Write(record)) {
					sync_cout << "info string Failed to write a record." << sync_endl;
					std::exit(1);
				}
			}

			progress_report.Show(global_position_index += records.size());

			need_wait = need_wait ||
				(progress_report.HasDataPerTime() &&
					progress_report.GetDataPerTime() * 2 < progress_report.GetMaxDataPerTime());

			if (need_wait) {
				// 処理速度が低下してきている。
				// 全てのスレッドを待機する。
#pragma omp barrier

				// マスタースレッドでしばらく待機する。
#pragma omp master
				{
					sync_cout << "Speed is down. Waiting for a while. GetDataPerTime()=" <<
						progress_report.GetDataPerTime() << " GetMaxDataPerTime()=" <<
						progress_report.GetMaxDataPerTime() << sync_endl;

					std::this_thread::sleep_for(std::chrono::minutes(10));
					progress_report.Reset();
					need_wait = false;
				}

				// マスタースレッドの待機が終わるまで、再度全てのスレッドを待機する。
#pragma omp barrier
			}
		}

		// 必要局面数生成したら全スレッドの探索を停止する
		// こうしないと相入玉等合法手の多い局面で止まるまでに時間がかかる
		Threads.stop = true;
	}

	std::cout << "Number of plays per record=" << global_position_index / num_records << std::endl;

	if (measure_depth) {
		char output_file_path[1024];
		std::sprintf(output_file_path, "%s/kifu.%s.%d.%I64d.%I64d.search_depth.csv", kifu_directory.c_str(),
			output_file_name_tag.c_str(), search_depth, num_positions, start_time);

		FILE* file = std::fopen(output_file_path, "wt");
		fprintf(file, "game_play,N,average,sd\n");

		for (int game_play = 0; game_play <= kMaxGamePlay; ++game_play) {
			double sum = 0.0;
			double sum2 = 0.0;
			int N = static_cast<int>(game_play_to_depths[game_play].size());
			for (auto search_depth : game_play_to_depths[game_play]) {
				sum += search_depth;
				sum2 += search_depth * search_depth;
			}

			if (N) {
				double average = sum / N;
				double sd = sum2 / N - average * average;
				fprintf(file, "%d,%d,%f,%f\n", game_play, N, average, sd);
			}
			else {
				fprintf(file, "%d,%d,%f,%f\n", game_play, N, 0.0, 0.0);
			}
		}
	}
}

void Tanuki::ConvertSfenToLearningData() {
	//Eval::load_eval();

	omp_set_num_threads((int)Options["Threads"]);

	Search::LimitsType limits;
	// 引き分けの手数付近で引き分けの値が返るのを防ぐため1 << 16にする
	limits.max_game_ply = 1 << 16;
	limits.depth = MAX_PLY;
	limits.silent = true;
	limits.enteringKingRule = EKR_27_POINT;
	Search::Limits = limits;

	std::string input_sfen_file_name =
		(std::string)Options[kOptionConvertSfenToLearningDataInputSfenFileName];
	int search_depth = Options[kOptionConvertSfenToLearningDataSearchDepth];
	std::string output_file_name = Options[kOptionConvertSfenToLearningDataOutputFileName];

	std::cout << "input_sfen_file_name=" << input_sfen_file_name << std::endl;
	std::cout << "search_depth=" << search_depth << std::endl;
	std::cout << "output_file_name=" << output_file_name << std::endl;

	time_t start_time;
	std::time(&start_time);

	std::vector<std::string> sfens;
	{
		std::ifstream ifs(input_sfen_file_name);
		std::string sfen;
		while (std::getline(ifs, sfen)) {
			sfens.push_back(sfen);
		}
	}

	// スレッド間で共有する
	std::atomic_int64_t global_sfen_index;
	global_sfen_index = 0;
	int64_t num_sfens = sfens.size();
	ProgressReport progress_report(num_sfens, 60);
	std::unique_ptr<KifuWriter> kifu_writer =
		std::make_unique<KifuWriter>(output_file_name);
	std::mutex mutex;
#pragma omp parallel
	{
		int thread_index = ::omp_get_thread_num();
		WinProcGroup::bindThisThread(thread_index);

		for (int64_t sfen_index = global_sfen_index++; sfen_index < num_sfens;
			sfen_index = global_sfen_index++) {
			const std::string& sfen = sfens[sfen_index];
			Thread& thread = *Threads[thread_index];
			StateInfo state_infos[4096] = {};
			StateInfo* state = state_infos + 8;
			Position& pos = thread.rootPos;
			pos.set_hirate(state, &thread);

			std::istringstream iss(sfen);
			// startpos moves 7g7f 3c3d 2g2f
			std::vector<Learner::PackedSfenValue> records;
			std::string token;
			Color win = COLOR_NB;
			while (iss >> token) {
				if (token == "startpos" || token == "moves") {
					continue;
				}

				Move m = USI::to_move(pos, token);
				if (!is_ok(m) || !pos.legal(m)) {
					break;
				}

				pos.do_move(m, state[pos.game_ply()]);

				Learner::search(pos, search_depth);
				const auto& root_moves = pos.this_thread()->rootMoves;
				const auto& root_move = root_moves[0];

				Learner::PackedSfenValue record = {};
				pos.sfen_pack(record.sfen);
				record.score = root_move.score;
				record.gamePly = pos.game_ply();
				records.push_back(record);

				if (pos.DeclarationWin()) {
					win = pos.side_to_move();
					break;
				}
			}

			// sync_cout << pos << sync_endl;
			// pos.DeclarationWin();

			if (win == COLOR_NB) {
				sync_cout << "Skipped..." << sync_endl;
				continue;
			}
			sync_cout << "DeclarationWin..." << sync_endl;

			int game_result = GameResultWin;
			for (int i = static_cast<int>(records.size()) - 1; i >= 0; --i) {
				records[i].game_result = game_result;
				game_result = -game_result;
			}

			std::lock_guard<std::mutex> lock_gurad(mutex);
			{
				for (const auto& record : records) {
					if (!kifu_writer->Write(record)) {
						sync_cout << "info string Failed to write a record." << sync_endl;
						std::exit(1);
					}
				}
			}

			progress_report.Show(global_sfen_index);
		}
	}
}

#endif
