//#pragma optimize( "", off )

#include "tanuki_book.h"
#include "config.h"

#ifdef EVAL_LEARN

#include <atomic>
#include <ctime>
#include <filesystem>
#include <fstream>
#include <queue>
#include <random>
#include <set>
#include <sstream>

#include <omp.h>

#include "evaluate.h"
#include "extra/book/book.h"
#include "learn/learn.h"
#include "misc.h"
#include "position.h"
#include "tanuki_progress_report.h"
#include "thread.h"
#include "tt.h"

using Book::BookMoveSelector;
using Book::BookPos;
using Book::MemoryBook;
using USI::Option;

namespace {
	constexpr const char* kBookSfenFile = "BookSfenFile";
	constexpr const char* kBookMaxMoves = "BookMaxMoves";
	constexpr const char* kBookFile = "BookFile";
	constexpr const char* kBookSearchDepth = "BookSearchDepth";
	constexpr const char* kBookSearchNodes = "BookSearchNodes";
	constexpr const char* kBookInputFile = "BookInputFile";
	constexpr const char* kBookInputFolder = "BookInputFolder";
	constexpr const char* kBookOutputFile = "BookOutputFile";
	constexpr const char* kThreads = "Threads";
	constexpr const char* kMultiPV = "MultiPV";
	constexpr const char* kBookOverwriteExistingPositions = "OverwriteExistingPositions";
	constexpr const char* kBookNarrowBook = "NarrowBook";
	constexpr const char* kBookTargetSfensFile = "BookTargetSfensFile";
	constexpr int kShowProgressPerAtMostSec = 1 * 60 * 60;	// 1時間
	constexpr time_t kSavePerAtMostSec = 6 * 60 * 60;		// 6時間

	struct SfenAndMove {
		std::string sfen;
		Move best_move;
		Move next_move;
	};

	void WriteBook(Book::MemoryBook& book, const std::string output_book_file_path) {
		std::string backup_file_path = output_book_file_path + ".bak";

		if (std::filesystem::exists(backup_file_path)) {
			sync_cout << "Removing the backup file. backup_file_path=" << backup_file_path << sync_endl;
			std::filesystem::remove(backup_file_path);
		}

		if (std::filesystem::exists(output_book_file_path)) {
			sync_cout << "Renaming the output file. output_book_file_path=" << output_book_file_path << " backup_file_path=" << backup_file_path << sync_endl;
			std::filesystem::rename(output_book_file_path, backup_file_path);
		}

		book.write_book(output_book_file_path);
		sync_cout << "|output_book_file_path|=" << book.get_body()->size() << sync_endl;
	}

	void WriteBook(BookMoveSelector& book, const std::string output_book_file_path) {
		WriteBook(book.get_body(), output_book_file_path);
	}

	std::mutex UPSERT_BOOK_MOVE_MUTEX;

	/// <summary>
	/// 定跡データベースに指し手を登録する。
	/// すでに指し手が登録されている場合、情報を上書きする。
	/// 内部的に大域的なロックを取って処理する。
	/// 指し手情報は16ビットに変換してから保存する。
	/// </summary>
	/// <param name="book"></param>
	/// <param name="sfen"></param>
	/// <param name="best_move"></param>
	/// <param name="next_move"></param>
	/// <param name="value"></param>
	/// <param name="depth"></param>
	/// <param name="num"></param>
	void UpsertBookMove(MemoryBook& book, const std::string& sfen, Move best_move, Move next_move, int value, int depth, uint64_t num)
	{
		std::lock_guard<std::mutex> lock(UPSERT_BOOK_MOVE_MUTEX);

		// MemoryBook::insert()に処理を移譲する。
		book.insert(sfen, BookPos(Move16(best_move), Move16(next_move), value, depth, num));
	}
}

bool Tanuki::InitializeBook(USI::OptionsMap& o) {
	o[kBookSfenFile] << Option("merged.sfen");
	o[kBookMaxMoves] << Option(32, 0, 256);
	o[kBookSearchDepth] << Option(64, 0, 256);
	o[kBookSearchNodes] << Option(500000 * 60, 0, INT_MAX);
	o[kBookInputFile] << Option("user_book1.db");
	o[kBookInputFolder] << Option("");
	o[kBookOutputFile] << Option("user_book2.db");
	o[kBookOverwriteExistingPositions] << Option(false);
	o[kBookTargetSfensFile] << Option("");
	return true;
}

// sfen棋譜ファイルから定跡データベースを作成する
bool Tanuki::CreateRawBook() {
	Search::LimitsType limits;
	// 引き分けの手数付近で引き分けの値が返るのを防ぐため1 << 16にする
	limits.max_game_ply = 1 << 16;
	limits.depth = MAX_PLY;
	limits.silent = true;
	limits.enteringKingRule = EKR_27_POINT;
	Search::Limits = limits;

	std::string sfen_file = Options[kBookSfenFile];
	int max_moves = (int)Options[kBookMaxMoves];
	bool narrow_book = static_cast<bool>(Options[kBookNarrowBook]);

	std::ifstream ifs(sfen_file);
	if (!ifs) {
		sync_cout << "info string Failed to open the sfen file." << sync_endl;
		std::exit(-1);
	}
	std::string line;

	MemoryBook memory_book;
	std::vector<StateInfo> state_info(1024);
	int num_records = 0;
	while (std::getline(ifs, line)) {
		std::istringstream iss(line);
		Position& pos = Threads[0]->rootPos;
		pos.set_hirate(&state_info[0], Threads[0]);

		std::string token;
		while (pos.game_ply() < max_moves && iss >> token) {
			if (token == "startpos" || token == "moves") {
				continue;
			}
			Move move = USI::to_move(pos, token);
			if (!pos.pseudo_legal(move) || !pos.legal(move)) {
				continue;
			}

			UpsertBookMove(memory_book, pos.sfen(), move, Move::MOVE_NONE, 0, 0, 1);

			pos.do_move(move, state_info[pos.game_ply()]);
		}

		++num_records;
		if (num_records % 10000 == 0) {
			sync_cout << "info string " << num_records << sync_endl;
		}
	}

	WriteBook(memory_book, "book/" + Options[kBookFile]);

	return true;
}

bool Tanuki::CreateScoredBook() {
	int num_threads = (int)Options[kThreads];
	std::string input_book_file = Options[kBookInputFile];
	int search_depth = (int)Options[kBookSearchDepth];
	int search_nodes = (int)Options[kBookSearchNodes];
	int multi_pv = (int)Options[kMultiPV];
	std::string output_book_file = Options[kBookOutputFile];
	bool overwrite_existing_positions = static_cast<bool>(Options[kBookOverwriteExistingPositions]);

	omp_set_num_threads(num_threads);

	sync_cout << "info string num_threads=" << num_threads << sync_endl;
	sync_cout << "info string input_book_file=" << input_book_file << sync_endl;
	sync_cout << "info string search_depth=" << search_depth << sync_endl;
	sync_cout << "info string search_nodes=" << search_nodes << sync_endl;
	sync_cout << "info string multi_pv=" << multi_pv << sync_endl;
	sync_cout << "info string output_book_file=" << output_book_file << sync_endl;
	sync_cout << "info string overwrite_existing_positions=" << overwrite_existing_positions
		<< sync_endl;

	Search::LimitsType limits;
	// 引き分けの手数付近で引き分けの値が返るのを防ぐため1 << 16にする
	limits.max_game_ply = 1 << 16;
	limits.depth = MAX_PLY;
	limits.silent = true;
	limits.enteringKingRule = EKR_27_POINT;
	Search::Limits = limits;

	MemoryBook input_book;
	input_book_file = "book/" + input_book_file;
	sync_cout << "Reading input book file: " << input_book_file << sync_endl;
	input_book.read_book(input_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|input_book|=" << input_book.get_body()->size() << sync_endl;

	MemoryBook output_book;
	output_book_file = "book/" + output_book_file;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book(output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body()->size() << sync_endl;

	std::vector<std::string> sfens;
	for (const auto& sfen_and_count : *input_book.get_body()) {
		if (!overwrite_existing_positions &&
			output_book.get_body()->find(sfen_and_count.first) != output_book.get_body()->end()) {
			continue;
		}
		sfens.push_back(sfen_and_count.first);
	}
	int num_sfens = sfens.size();
	sync_cout << "Number of the positions to be processed: " << num_sfens << sync_endl;

	time_t start_time = 0;
	std::time(&start_time);

	std::atomic_int global_position_index;
	global_position_index = 0;
	ProgressReport progress_report(num_sfens, kShowProgressPerAtMostSec);
	time_t last_save_time_sec = std::time(nullptr);

	std::atomic<bool> need_wait = false;
	std::atomic_int global_pos_index;
	global_pos_index = 0;
	std::atomic_int global_num_processed_positions;
	global_num_processed_positions = 0;
#pragma omp parallel
	{
		int thread_index = ::omp_get_thread_num();
		WinProcGroup::bindThisThread(thread_index);

		for (int position_index = global_pos_index++; position_index < num_sfens;
			position_index = global_pos_index++) {
			const std::string& sfen = sfens[position_index];
			Thread& thread = *Threads[thread_index];
			StateInfo state_info = {};
			Position& pos = thread.rootPos;
			pos.set(sfen, &state_info, &thread);

			if (pos.is_mated()) {
				continue;
			}

			Learner::search(pos, search_depth, multi_pv, search_nodes);

			int num_pv = std::min(multi_pv, static_cast<int>(thread.rootMoves.size()));
			for (int pv_index = 0; pv_index < num_pv; ++pv_index) {
				const auto& root_move = thread.rootMoves[pv_index];
				Move best = Move::MOVE_NONE;
				if (root_move.pv.size() >= 1) {
					best = root_move.pv[0];
				}
				Move next = Move::MOVE_NONE;
				if (root_move.pv.size() >= 2) {
					next = root_move.pv[1];
				}
				int value = root_move.score;
				UpsertBookMove(output_book, sfen, best, next, value, thread.completedDepth, 1);
			}

			int num_processed_positions = ++global_num_processed_positions;
			// 念のため、I/Oはマスタースレッドでのみ行う
#pragma omp master
			{
				// 進捗状況を表示する
				progress_report.Show(num_processed_positions);

				// 一定時間ごとに保存する
				{
					std::lock_guard<std::mutex> lock(UPSERT_BOOK_MOVE_MUTEX);
					if (last_save_time_sec + kSavePerAtMostSec < std::time(nullptr)) {
						WriteBook(output_book, output_book_file);
						last_save_time_sec = std::time(nullptr);
					}
				}
			}

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
	}

	WriteBook(output_book, output_book_file);

	return true;
}

// 複数の定跡をマージする
// BookInputFileには「;」区切りで定跡データベースのフルパスを指定する
// BookOutputFileにはbook以下のファイル名を指定する
bool Tanuki::MergeBook() {
	std::string input_file_list = Options[kBookInputFile];
	std::string output_file = Options[kBookOutputFile];

	sync_cout << "info string input_file_list=" << input_file_list << sync_endl;
	sync_cout << "info string output_file=" << output_file << sync_endl;

	BookMoveSelector output_book;
	sync_cout << "Reading output book file: " << output_file << sync_endl;
	output_book.get_body().read_book("book/" + output_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body().get_body()->size() << sync_endl;

	std::vector<std::string> input_files;
	{
		std::istringstream iss(input_file_list);
		std::string input_file;
		while (std::getline(iss, input_file, ';')) {
			input_files.push_back(input_file);
		}
	}

	for (int input_file_index = 0; input_file_index < input_files.size(); ++input_file_index) {
		sync_cout << (input_file_index + 1) << " / " << input_files.size() << sync_endl;

		const auto& input_file = input_files[input_file_index];
		BookMoveSelector input_book;
		sync_cout << "Reading input book file: " << input_file << sync_endl;
		input_book.get_body().read_book(input_file);
		sync_cout << "done..." << sync_endl;
		sync_cout << "|input_book|=" << input_book.get_body().get_body()->size() << sync_endl;

		for (const auto& book_type : *input_book.get_body().get_body()) {
			const auto& sfen = book_type.first;
			const auto& pos_move_list = book_type.second;

			uint64_t max_num = 0;
			for (const auto& pos_move : *pos_move_list) {
				max_num = std::max(max_num, pos_move.num);
			}

			for (const auto& pos_move : *pos_move_list) {
				if (max_num > 0 && pos_move.num == 0) {
					// 採択回数が設定されており、この手の採択回数が0の場合、
					// 手動でこの手を指さないよう調整されている。
					// そのような手はスキップする。
					continue;
				}
				output_book.get_body().insert(sfen, pos_move, false);
			}
		}
	}

	WriteBook(output_book, "book/" + output_file);

	return true;
}

// 定跡の各指し手に評価値を付ける
// 相手の応手が設定されていない場合、読み筋から設定する。
bool Tanuki::SetScoreToMove() {
	int num_threads = (int)Options[kThreads];
	std::string input_book_file = Options[kBookInputFile];
	int search_depth = (int)Options[kBookSearchDepth];
	int search_nodes = (int)Options[kBookSearchNodes];
	std::string output_book_file = Options[kBookOutputFile];

	omp_set_num_threads(num_threads);

	sync_cout << "info string num_threads=" << num_threads << sync_endl;
	sync_cout << "info string input_book_file=" << input_book_file << sync_endl;
	sync_cout << "info string search_depth=" << search_depth << sync_endl;
	sync_cout << "info string search_nodes=" << search_nodes << sync_endl;
	sync_cout << "info string output_book_file=" << output_book_file << sync_endl;

	Search::LimitsType limits;
	// 引き分けの手数付近で引き分けの値が返るのを防ぐため1 << 16にする
	limits.max_game_ply = 1 << 16;
	limits.depth = MAX_PLY;
	limits.silent = true;
	limits.enteringKingRule = EKR_27_POINT;
	Search::Limits = limits;

	MemoryBook input_book;
	input_book_file = "book/" + input_book_file;
	sync_cout << "Reading input book file: " << input_book_file << sync_endl;
	input_book.read_book(input_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|input_book|=" << input_book.get_body()->size() << sync_endl;

	MemoryBook output_book;
	output_book_file = "book/" + output_book_file;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book(output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body()->size() << sync_endl;

	std::vector<SfenAndMove> sfen_and_moves;

	// 未処理の局面をキューに登録する
	for (const auto& input_sfen_and_pos_move_list : *input_book.get_body()) {
		const auto& input_sfen = input_sfen_and_pos_move_list.first;

		Position& position = Threads[0]->rootPos;
		StateInfo state_info;
		position.set(input_sfen, &state_info, Threads[0]);

		auto output_book_moves_it = output_book.get_body()->find(input_sfen);
		if (output_book_moves_it == output_book.get_body()->end()) {
			// 出力定跡データベースに局面が登録されていなかった場合
			for (const auto& input_book_move : *input_sfen_and_pos_move_list.second) {
				sfen_and_moves.push_back({
					input_sfen,
					position.to_move(input_book_move.bestMove),
					position.to_move(input_book_move.nextMove)
					});
			}
			continue;
		}

		const auto& output_book_moves = *output_book_moves_it->second;
		for (const auto& input_book_move : *input_sfen_and_pos_move_list.second) {
			if (std::find_if(output_book_moves.begin(), output_book_moves.end(),
				[&input_book_move](const auto& x) {
					return input_book_move.bestMove == x.bestMove;
				}) != output_book_moves.end()) {
				continue;
			}

			// 出力定跡データベースに指し手が登録されていなかった場合
			sfen_and_moves.push_back({
				input_sfen,
				position.to_move(input_book_move.bestMove),
				position.to_move(input_book_move.nextMove)
				});
		}
	}
	int num_sfen_and_moves = sfen_and_moves.size();
	sync_cout << "Number of the moves to be processed: " << num_sfen_and_moves << sync_endl;

	// マルチスレッド処理準備のため、インデックスを初期化する
	std::atomic_int global_sfen_and_move_index;
	global_sfen_and_move_index = 0;

	// 進捗状況表示の準備
	ProgressReport progress_report(num_sfen_and_moves, kShowProgressPerAtMostSec);

	// 定跡を定期的に保存するための変数
	time_t last_save_time_sec = std::time(nullptr);

	// Apery式マルチスレッド処理
	std::atomic<bool> need_wait = false;
	std::atomic_int global_num_processed_positions;
	global_num_processed_positions = 0;
#pragma omp parallel
	{
		int thread_index = ::omp_get_thread_num();
		WinProcGroup::bindThisThread(thread_index);

		for (int position_index = global_sfen_and_move_index++; position_index < num_sfen_and_moves;
			position_index = global_sfen_and_move_index++) {
			const auto& sfen_and_move = sfen_and_moves[position_index];
			const auto& sfen = sfen_and_move.sfen;
			Thread& thread = *Threads[thread_index];
			StateInfo state_info = {};
			Position& pos = thread.rootPos;
			pos.set(sfen, &state_info, &thread);
			Move best_move = sfen_and_move.best_move;
			Move next_move = MOVE_NONE;

			if (pos.is_mated()) {
				continue;
			}

			if (!pos.pseudo_legal(best_move) || !pos.legal(best_move)) {
				sync_cout << "Illegal move. sfen=" << sfen << " best_move=" <<
					USI::move(best_move) << " next_move=" << USI::move(next_move) << sync_endl;
				continue;
			}

			StateInfo state_info0;
			pos.do_move(best_move, state_info0);
			Eval::evaluate_with_no_return(pos);

			// この局面について探索する
			auto value_and_pv = Learner::search(pos, search_depth, 1, search_nodes);

			// ひとつ前の局面から見た評価値を代入する必要があるので、符号を反転する。
			Value value = -value_and_pv.first;

			auto pv = value_and_pv.second;
			if (next_move == MOVE_NONE && pv.size() >= 1) {
				// 定跡の手を指した次の局面なので、nextMoveにはpv[0]を代入する。
				// ただし、もともとnextMoveが設定されている場合、それを優先する。
				next_move = pv[0];
			}

			Depth depth = pos.this_thread()->completedDepth;

			// 指し手を出力先の定跡に登録する
			UpsertBookMove(output_book, sfen, best_move, next_move, value, depth, 1);

			int num_processed_positions = ++global_num_processed_positions;
			// 念のため、I/Oはマスタースレッドでのみ行う
#pragma omp master
			{
				// 進捗状況を表示する
				progress_report.Show(num_processed_positions);

				// 定跡をストレージに書き出す。
				if (last_save_time_sec + kSavePerAtMostSec < std::time(nullptr)) {
					std::lock_guard<std::mutex> lock(UPSERT_BOOK_MOVE_MUTEX);
					WriteBook(output_book, output_book_file);
					last_save_time_sec = std::time(nullptr);
				}
			}

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
	}

	WriteBook(output_book, output_book_file);

	return true;
}

namespace {
	struct ValueMoveDepth {
		// 評価値
		Value value = VALUE_NONE;
		// この局面における指し手
		// 定跡の指し手に対する応手を定跡データベースに登録するため、指し手も返せるようにしておく
		Move move = MOVE_NONE;
		// 探索深さ
		Depth depth = DEPTH_NONE;
	};

	// (sfen, ValueAndDepth)
	// やねうら王本家では手数の部分を削除しているが、
	// 実装を容易にするため手数も含めて格納する。
	// 本来であればこれによる副作用を熟考する必要があるが、
	// よく分からなかったので適当に実装する。
	std::unordered_map<std::string, ValueMoveDepth> memo;

	// IgnoreBookPlyオプションがtrueの場合、
	// 与えられたsfen文字列からplyを取り除く
	std::string RemovePlyIfIgnoreBookPly(const std::string& sfen) {
		if (Options["IgnoreBookPly"]) {
			return StringExtension::trim_number(sfen);
		}
		else {
			return sfen;
		}
	}

	// Nega-Max法で末端局面の評価値をroot局面に向けて伝搬する
	// book 定跡データベース
	// pos 現在の局面
	// value_and_depth_without_nega_max_parent SetScoreToMove()で設定した評価値と探索深さ
	//                                         現局面から見た評価値になるよう、符号を反転してから渡すこと
	ValueMoveDepth NegaMax(MemoryBook& book, Position& pos, int& counter) {
		if (++counter % 1000000 == 0) {
			sync_cout << counter << " |memo|=" << memo.size() << sync_endl;
		}

		// この局面に対してのキーとして使用するsfen文字列。
		// 必要に応じて末尾のplyを取り除いておく。
		std::string sfen = RemovePlyIfIgnoreBookPly(pos.sfen());

		// NegaMax()は定跡データベースに登録されている局面についてのみ処理を行う。
		ASSERT_LV3(book.get_body()->count(sfen));

		auto& vmd = memo[sfen];
		if (vmd.depth != DEPTH_NONE) {
			// キャッシュにヒットした場合、その値を返す。
			return vmd;
		}

		if (pos.is_mated()) {
			// 詰んでいる場合
			vmd.value = mated_in(0);
			vmd.depth = 0;
			return vmd;
		}

		if (pos.DeclarationWin() != MOVE_NONE) {
			// 宣言勝ちできる場合
			vmd.value = mate_in(1);
			vmd.depth = 0;
			return vmd;
		}

		auto repetition_state = pos.is_repetition();
		switch (repetition_state) {
		case REPETITION_WIN:
			// 連続王手の千日手により勝ちの場合
			vmd.value = mate_in(MAX_PLY);
			vmd.depth = 0;
			return vmd;

		case REPETITION_LOSE:
			// 連続王手の千日手により負けの場合
			vmd.value = mated_in(MAX_PLY);
			vmd.depth = 0;
			return vmd;

		case REPETITION_DRAW:
			// 引き分けの場合
			// やねうら王では先手後手に別々の評価値を付加している。
			// ここでは簡単のため、同じ評価値を付加する。
			vmd.value = static_cast<Value>(Options["Contempt"] * Eval::PawnValue / 100);
			vmd.depth = 0;
			return vmd;

		case REPETITION_SUPERIOR:
			// 優等局面の場合
			vmd.value = VALUE_SUPERIOR;
			vmd.depth = 0;
			return vmd;

		case REPETITION_INFERIOR:
			// 劣等局面の場合
			vmd.value = -VALUE_SUPERIOR;
			vmd.depth = 0;
			return vmd;

		default:
			break;
		}

		vmd.value = mated_in(0);
		vmd.depth = 0;
		// 全合法手について調べる
		for (const auto& move : MoveList<LEGAL_ALL>(pos)) {
			StateInfo state_info = {};
			pos.do_move(move, state_info);
			if (book.get_body()->find(pos.sfen()) != book.get_body()->end()) {
				// 子局面が存在する場合のみ処理する。
				ValueMoveDepth vmd_child = NegaMax(book, pos, counter);

				// 指し手情報に探索の結果を格納する。
				// 返ってきた評価値は次の局面から見た評価値なので、符号を反転する。
				// また、探索深さを+1する。
				UpsertBookMove(book, sfen, move, vmd_child.move, -vmd_child.value, vmd_child.depth + 1, 1);
			}
			pos.undo_move(move);
		}

		// 現局面について、定跡データベースに子局面への指し手が登録された。
		// 定跡データベースを調べ、この局面における最適な指し手を調べて返す。
		auto book_moves = book.get_body()->find(sfen);
		ASSERT_LV3(book_moves != book.get_body()->end());
		for (const auto& book_move : *book_moves->second) {
			if (vmd.value < book_move.value) {
				vmd.value = static_cast<Value>(book_move.value);
				vmd.move = pos.to_move(book_move.bestMove);
				vmd.depth = book_move.depth;
			}
		}

		return vmd;
	}
}

// 定跡データベースの末端局面の評価値をroot局面に向けて伝搬する
bool Tanuki::PropagateLeafNodeValuesToRoot() {
	std::string input_book_file = Options[kBookInputFile];
	std::string output_book_file = Options[kBookOutputFile];

	sync_cout << "info string input_book_file=" << input_book_file << sync_endl;
	sync_cout << "info string output_book_file=" << output_book_file << sync_endl;

	Search::LimitsType limits;
	// 引き分けの手数付近で引き分けの値が返るのを防ぐため1 << 16にする
	limits.max_game_ply = 1 << 16;
	limits.depth = MAX_PLY;
	limits.silent = true;
	limits.enteringKingRule = EKR_27_POINT;
	Search::Limits = limits;

	MemoryBook book;
	input_book_file = "book/" + input_book_file;
	sync_cout << "Reading input book file: " << input_book_file << sync_endl;
	book.read_book(input_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|input_book_file|=" << book.get_body()->size() << sync_endl;

	// 平手の局面からたどれる局面について処理する
	// メモをクリアするのを忘れない
	memo.clear();
	auto& pos = Threads[0]->rootPos;
	StateInfo state_info = {};
	pos.set_hirate(&state_info, Threads[0]);
	int counter = 0;
	NegaMax(book, pos, counter);

	// 平手の局面から辿れなかった局面を処理する
	for (const auto& book_entry : *book.get_body()) {
		auto& pos = Threads[0]->rootPos;
		StateInfo state_info = {};
		pos.set(book_entry.first, &state_info, Threads[0]);
		NegaMax(book, pos, counter);
	}

	WriteBook(book, "book/" + output_book_file);
	sync_cout << "|output_book|=" << book.get_body()->size() << sync_endl;

	return true;
}

namespace {
	// 与えられた局面における、与えられた指し手が定跡データベースに含まれているかどうかを返す。
	// 含まれている場合は、その指し手へのポインターを返す。
	// 含まれていない場合は、nullptrを返す。
	BookPos* IsBookMoveExist(BookMoveSelector& book, Position& position, Move move) {
		auto book_moves = book.get_body().find(position);
		if (book_moves == nullptr) {
			return nullptr;
		}

		auto book_move = std::find_if(book_moves->begin(), book_moves->end(),
			[move](auto& m) {
				return m.bestMove == Move16(move);
			});
		if (book_move == book_moves->end()) {
			// 定跡データベースに、この指し手が登録されていない場合。
			return nullptr;
		}
		else {
			return &*book_move;
		}
	}

	// 与えられた局面における、与えられた指し手が、展開すべき対象かどうかを返す。
	// 展開する条件は、
	// - 定跡データベースに指し手が含まれている、かつ評価値が閾値以上
	// - 定跡データベースに指し手が含まれていない、かつ次の局面が含まれている
	bool IsTargetMove(BookMoveSelector& book, Position& position, Move move32, int book_eval_black_limit, int book_eval_white_limit) {
		auto book_pos = IsBookMoveExist(book, position, move32);
		if (book_pos != nullptr) {
			// 定跡データベースに指し手が含まれている
			int book_eval_limit = position.side_to_move() == BLACK ? book_eval_black_limit : book_eval_white_limit;
			return book_pos->value > book_eval_limit;
		}
		else {
			StateInfo state_info = {};
			position.do_move(move32, state_info);
			bool exist = book.get_body().get_body()->find(position.sfen()) != book.get_body().get_body()->end();
			position.undo_move(move32);
			return exist;
		}
	}

	// 与えられた局面を延長すべきかどうか判断する
	bool IsTargetPosition(BookMoveSelector& book, Position& position, int multi_pv) {
		auto book_moves = book.get_body().find(position);
		if (book_moves == nullptr) {
			// 定跡データベースに、この局面が登録されていない場合、延長する。
			return true;
		}

		// 登録されている指し手の数が、MultiPVより少ない場合、延長する。
		return book_moves->size() < multi_pv;
	}
}

// 定跡の延長の対象となる局面を抽出する。
// MultiPVが指定された値より低い局面、および定跡データベースに含まれていない局面が対象となる。
// あらかじめ、SetScoreToMove()を用い、定跡データベースの各指し手に評価値を付け、
// PropagateLeafNodeValuesToRoot()を用い、Leaf局面の評価値をRoot局面に伝搬してから使用する。
bool Tanuki::ExtractTargetPositions() {
	int multi_pv = (int)Options[kMultiPV];
	std::string input_book_file = Options[kBookInputFile];
	int book_eval_black_limit = (int)Options["BookEvalBlackLimit"];
	int book_eval_white_limit = (int)Options["BookEvalWhiteLimit"];
	std::string target_sfens_file = Options[kBookTargetSfensFile];

	sync_cout << "info string multi_pv=" << multi_pv << sync_endl;
	sync_cout << "info string input_book_file=" << input_book_file << sync_endl;
	sync_cout << "info string book_eval_black_limit=" << book_eval_black_limit << sync_endl;
	sync_cout << "info string book_eval_white_limit=" << book_eval_white_limit << sync_endl;
	sync_cout << "info string target_sfens_file=" << sync_endl;

	Search::LimitsType limits;
	// 引き分けの手数付近で引き分けの値が返るのを防ぐため1 << 16にする
	limits.max_game_ply = 1 << 16;
	limits.depth = MAX_PLY;
	limits.silent = true;
	limits.enteringKingRule = EKR_27_POINT;
	Search::Limits = limits;

	BookMoveSelector book;
	input_book_file = "book/" + input_book_file;
	sync_cout << "Reading input book file: " << input_book_file << sync_endl;
	book.get_body().read_book(input_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|input_book_file|=" << book.get_body().get_body()->size() << sync_endl;

	std::set<std::string> explorered;
	explorered.insert(SFEN_HIRATE);
	// 千日手の処理等のため、平手局面からの指し手として保持する
	// Moveは32ビット版とする
	std::deque<std::vector<Move>> frontier;
	frontier.push_back({});

	std::ofstream ofs(target_sfens_file);

	int counter = 0;
	std::vector<StateInfo> state_info(1024);
	while (!frontier.empty()) {
		if (++counter % 1000 == 0) {
			sync_cout << counter << sync_endl;
		}

		auto moves = frontier.front();
		Position& position = Threads[0]->rootPos;
		position.set_hirate(&state_info[0], Threads[0]);
		// 現局面まで指し手を進める
		for (auto move : moves) {
			position.do_move(move, state_info[position.game_ply()]);
		}
		frontier.pop_front();

		// 千日手の局面は処理しない
		auto draw_type = position.is_repetition(MAX_PLY);
		if (draw_type == REPETITION_DRAW) {
			continue;
		}

		// 詰み、宣言勝ちの局面も処理しない
		if (position.is_mated() || position.DeclarationWin() != MOVE_NONE) {
			continue;
		}

		if (IsTargetPosition(book, position, multi_pv)) {
			// 対象の局面を書き出す。
			// 形式はmoveをスペース区切りで並べたものとする。
			// これは、千日手等を認識させるため。

			for (auto m : moves) {
				ofs << m << " ";
			}
			ofs << std::endl;
		}

		// 子局面を展開する
		for (const auto& move : MoveList<LEGAL_ALL>(position)) {
			if (!position.pseudo_legal(move) || !position.legal(move)) {
				// 不正な手の場合は処理しない
				continue;
			}

			if (!IsTargetMove(book, position, move, book_eval_black_limit, book_eval_white_limit)) {
				// この指し手の先の局面は処理しない。
				continue;
			}

			position.do_move(move, state_info[position.game_ply()]);

			// undo_move()を呼び出す必要があるので、continueとbreakを禁止する。
			if (!explorered.count(position.sfen())) {
				explorered.insert(position.sfen());

				moves.push_back(move);
				frontier.push_back(moves);
				moves.pop_back();
			}

			position.undo_move(move);
		}
	}

	sync_cout << "done..." << sync_endl;

	return true;
}

bool Tanuki::AddTargetPositions() {
	int num_threads = (int)Options[kThreads];
	std::string input_book_file = Options[kBookInputFile];
	int search_depth = (int)Options[kBookSearchDepth];
	int search_nodes = (int)Options[kBookSearchNodes];
	int multi_pv = (int)Options[kMultiPV];
	std::string output_book_file = Options[kBookOutputFile];
	std::string target_sfens_file = Options[kBookTargetSfensFile];

	omp_set_num_threads(num_threads);

	sync_cout << "info string num_threads=" << num_threads << sync_endl;
	sync_cout << "info string input_book_file=" << input_book_file << sync_endl;
	sync_cout << "info string search_depth=" << search_depth << sync_endl;
	sync_cout << "info string search_nodes=" << search_nodes << sync_endl;
	sync_cout << "info string multi_pv=" << multi_pv << sync_endl;
	sync_cout << "info string output_book_file=" << output_book_file << sync_endl;
	sync_cout << "info string target_sfens_file=" << sync_endl;

	Search::LimitsType limits;
	// 引き分けの手数付近で引き分けの値が返るのを防ぐため1 << 16にする
	limits.max_game_ply = 1 << 16;
	limits.depth = MAX_PLY;
	limits.silent = true;
	limits.enteringKingRule = EKR_27_POINT;
	Search::Limits = limits;

	MemoryBook input_book;
	input_book_file = "book/" + input_book_file;
	sync_cout << "Reading input book file: " << input_book_file << sync_endl;
	input_book.read_book(input_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|input_book|=" << input_book.get_body()->size() << sync_endl;

	MemoryBook output_book;
	output_book_file = "book/" + output_book_file;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book(output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body()->size() << sync_endl;

	// 対象の局面を読み込む
	sync_cout << "Reading target positions: target_sfens_file=" << target_sfens_file << sync_endl;
	std::vector<std::string> lines;
	std::ifstream ifs(target_sfens_file);
	std::string line;
	while (std::getline(ifs, line)) {
		lines.push_back(line);
	}
	int num_positions = lines.size();
	sync_cout << "done..." << sync_endl;
	sync_cout << "|lines|=" << num_positions << sync_endl;

	ProgressReport progress_report(num_positions, kShowProgressPerAtMostSec);
	time_t last_save_time_sec = std::time(nullptr);

	std::atomic<bool> need_wait = false;
	std::atomic_int global_pos_index;
	global_pos_index = 0;
	std::atomic_int global_num_processed_positions;
	global_num_processed_positions = 0;

#pragma omp parallel
	{
		int thread_index = ::omp_get_thread_num();
		WinProcGroup::bindThisThread(thread_index);

		for (int position_index = global_pos_index++; position_index < num_positions;
			position_index = global_pos_index++) {
			Thread& thread = *Threads[thread_index];
			std::vector<StateInfo> state_info(1024);
			Position& pos = thread.rootPos;
			pos.set_hirate(&state_info[0], &thread);

			std::istringstream iss(lines[position_index]);
			std::string move_string;
			while (iss >> move_string) {
				Move16 move16 = USI::to_move16(move_string);
				Move move = pos.to_move(move16);
				pos.do_move(move, state_info[pos.game_ply()]);
			}

			if (pos.is_mated()) {
				continue;
			}

			Learner::search(pos, search_depth, multi_pv, search_nodes);

			int num_pv = std::min(multi_pv, static_cast<int>(thread.rootMoves.size()));
			for (int pv_index = 0; pv_index < num_pv; ++pv_index) {
				const auto& root_move = thread.rootMoves[pv_index];
				Move best = Move::MOVE_NONE;
				if (root_move.pv.size() >= 1) {
					best = root_move.pv[0];
				}
				Move next = Move::MOVE_NONE;
				if (root_move.pv.size() >= 2) {
					next = root_move.pv[1];
				}
				int value = root_move.score;
				UpsertBookMove(output_book, pos.sfen(), best, next, value, thread.completedDepth, 1);
			}

			int num_processed_positions = ++global_num_processed_positions;
			// 念のため、I/Oはマスタースレッドでのみ行う
#pragma omp master
			{
				// 進捗状況を表示する
				progress_report.Show(num_processed_positions);

				// 一定時間ごとに保存する
				{
					std::lock_guard<std::mutex> lock(UPSERT_BOOK_MOVE_MUTEX);
					if (last_save_time_sec + kSavePerAtMostSec < std::time(nullptr)) {
						WriteBook(output_book, output_book_file);
						last_save_time_sec = std::time(nullptr);
					}
				}
			}

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

			// 置換表の世代を進める
			Threads[thread_index]->tt.new_search();
		}
	}

	WriteBook(output_book, output_book_file);

	return true;
}

bool Tanuki::CreateFromTanukiColiseum()
{
	std::string input_book_file = Options[kBookInputFile];
	std::string input_book_folder = Options[kBookInputFolder];
	std::string output_book_file = Options[kBookOutputFile];

	sync_cout << "info string input_book_file=" << input_book_file << sync_endl;
	sync_cout << "info string input_book_folder=" << input_book_folder << sync_endl;
	sync_cout << "info string output_book_file=" << output_book_file << sync_endl;

	Search::LimitsType limits;
	// 引き分けの手数付近で引き分けの値が返るのを防ぐため1 << 16にする
	limits.max_game_ply = 1 << 16;
	limits.depth = MAX_PLY;
	limits.silent = true;
	limits.enteringKingRule = EKR_27_POINT;
	Search::Limits = limits;

	MemoryBook input_book;
	input_book_file = "book/" + input_book_file;
	sync_cout << "Reading input book file: " << input_book_file << sync_endl;
	input_book.read_book(input_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|input_book|=" << input_book.get_body()->size() << sync_endl;

	MemoryBook output_book;
	output_book_file = "book/" + output_book_file;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book(output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body()->size() << sync_endl;

	// TanukiColiseu
	for (const auto& entry : std::filesystem::recursive_directory_iterator(input_book_folder)) {
		const auto& file_name = entry.path().filename();
		if (file_name != "sfen.txt") {
			continue;
		}

		std::cout << entry.path() << std::endl;

		std::fstream iss(entry.path());
		std::string line;
		while (std::getline(iss, line)) {
			auto& pos = Threads[0]->rootPos;
			std::vector<StateInfo> state_info(1024);
			pos.set_hirate(&state_info[0], Threads[0]);

			std::istringstream iss_line(line);
			std::string move_string;
			std::vector<std::pair<std::string, Move16>> moves;
			// 先手が勝った場合は0、後手が勝った場合は1
			// 引き分けの場合は-1
			int winner_offset = -1;
			while (iss_line >> move_string) {
				if (move_string == "startpos" || move_string == "moves") {
					continue;
				}

				auto sfen = pos.sfen();
				auto move = USI::to_move(pos, move_string);
				pos.do_move(move, state_info[pos.game_ply()]);

				auto move16 = USI::to_move16(move_string);
				moves.emplace_back(sfen, move16);

				auto repetition = pos.is_repetition();
				if (pos.is_mated()) {
					if (moves.size() % 2 == 0) {
						// 後手勝ち
						winner_offset = 1;
					}
					else {
						// 先手勝ち
						winner_offset = 0;
					}
					break;
				}
				else if (pos.DeclarationWin()) {
					if (moves.size() % 2 == 0) {
						// 先手勝ち
						winner_offset = 0;
					}
					else {
						// 後手勝ち
						winner_offset = 1;
					}
					break;
				}
				else if (repetition == REPETITION_WIN) {
					// 連続王手の千日手による勝ち
					if (moves.size() % 2 == 0) {
						// 先手勝ち
						winner_offset = 0;
					}
					else {
						// 後手勝ち
						winner_offset = 1;
					}
					break;
				}
				else if (repetition == REPETITION_LOSE) {
					// 連続王手の千日手による負け
					if (moves.size() % 2 == 0) {
						// 後手勝ち
						winner_offset = 1;
					}
					else {
						// 先手勝ち
						winner_offset = 0;
					}
					break;
				}
				else if (repetition == REPETITION_DRAW) {
					// 連続王手ではない普通の千日手
					break;
				}
				else if (repetition == REPETITION_SUPERIOR) {
					// 優等局面(盤上の駒が同じで手駒が相手より優れている)
					if (moves.size() % 2 == 0) {
						// 先手勝ち
						winner_offset = 0;
					}
					else {
						// 後手勝ち
						winner_offset = 1;
					}
					break;
				}
				else if (repetition == REPETITION_INFERIOR) {
					// 劣等局面(盤上の駒が同じで手駒が相手より優れている)
					if (moves.size() % 2 == 0) {
						// 後手勝ち
						winner_offset = 1;
					}
					else {
						// 先手勝ち
						winner_offset = 0;
					}
					break;
				}
			}

			if (winner_offset == -1) {
				continue;
			}

			for (int move_index = winner_offset; move_index < moves.size(); move_index += 2) {
				const auto& sfen = moves[move_index].first;
				Move16 best = moves[move_index].second;
				Move16 next = MOVE_NONE;
				if (move_index + 1 < moves.size()) {
					next = moves[move_index + 1].second;
				}

				output_book.insert(sfen, BookPos(best, next, 0, 0, 1));
			}
		}
	}

	WriteBook(output_book, output_book_file);

	return true;
}

bool Tanuki::ConvertToSfenEpd()
{
	std::string input_book_file = Options[kBookInputFile];
	std::string output_book_file = Options[kBookOutputFile];

	sync_cout << "info string input_book_file=" << input_book_file << sync_endl;
	sync_cout << "info string output_book_file=" << output_book_file << sync_endl;

	std::ifstream ifs(input_book_file);
	std::string line;
	std::ofstream ofs(output_book_file);
	while (std::getline(ifs, line)) {
		auto& pos = Threads[0]->rootPos;
		std::vector<StateInfo> state_info(1024);
		pos.set_hirate(&state_info[0], Threads[0]);

		std::istringstream iss_line(line);
		std::string move_string;
		int num_moves = 0;
		while (num_moves < 24) {
			if (!(iss_line >> move_string)) {
				break;
			}

			if (move_string == "startpos" || move_string == "moves") {
				continue;
			}

			auto sfen = pos.sfen();
			auto move = USI::to_move(pos, move_string);
			pos.do_move(move, state_info[pos.game_ply()]);
			++num_moves;
		}

		ofs << pos.sfen() << std::endl;
	}

	return true;
}

#endif
