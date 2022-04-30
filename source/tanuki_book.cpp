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
#include <regex>

#include <omp.h>

#include "sqlite/sqlite3.h"

#include "book/book.h"
#include "csa.h"
#include "evaluate.h"
#include "learn/learn.h"
#include "misc.h"
#include "position.h"
#include "tanuki_progress_report.h"
#include "thread.h"
#include "tt.h"

using Book::BookMoveSelector;
using Book::BookMove;
using Book::MemoryBook;
using USI::Option;

void position_cmd(Position& pos, std::istringstream& is, StateListPtr& states);
void go_cmd(const Position& pos, std::istringstream& is, StateListPtr& states, bool ignore_ponder = false);

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
	constexpr const char* kBookCsaFolder = "BookCsaFolder";
	constexpr const char* kBookTanukiColiseumLogFolder = "BookTanukiColiseumLogFolder";
	constexpr const char* kBookMinimumWinningPercentage = "BookMinimumWinningPercentage";
	constexpr const char* kBookBlackMinimumValue = "BookBlackMinimumValue";
	constexpr const char* kBookWhiteMinimumValue = "BookWhiteMinimumValue";
	constexpr const char* kBookMinimumCount = "BookMinimumCount";
	constexpr const char* kBookMinimumRating = "BookMinimumRating";
	constexpr const char* kBookUctUcb1Constant = "BookUctUcb1Constant";
	constexpr const char* kBookUctTimeMs = "BookUctTimeMs";
	constexpr const char* kBookUctIncMs = "BookUctIncMs";
	constexpr const char* kBookUctNumMatches = "BookUctNumMatches";
	constexpr const char* kBookUctMaxSearchPerPosition = "BookUctMaxSearchPerPosition";
	constexpr const char* kBookUctRecordFile = "BookUctRecordFile";
	constexpr int kShowProgressPerAtMostSec = 1 * 60 * 60;	// 1時間
	constexpr time_t kSavePerAtMostSec = 1 * 60 * 60;		// 1時間

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
		sync_cout << "|output_book_file_path|=" << book.get_body().size() << sync_endl;
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
		book.insert(sfen, BookMove(Move16(best_move), Move16(next_move), value, depth, num));
	}

	struct Player {
		std::string name;
		int rate;
	};

	bool ReadStrongPlayers(std::vector<Player>& strong_players) {
		std::ifstream ifs("players-floodgate.html");
		if (!ifs) {
			return false;
		}

		std::string name;
		int rate = 0;
		std::string line;
		while (std::getline(ifs, line)) {
			if (line.find("<a id=\"popup") != std::string::npos) {
				auto left = line.find(">");
				auto right = line.find("<", left);
				name = line.substr(left + 1, right - left - 1);
			}
			else if (line.find("<span id=\"popup") != std::string::npos) {
				auto left = line.find(">");
				auto right = line.find("<", left);
				auto rate_string = line.substr(left + 2, right - left - 2);
				// N/A は取り除く
				if (!std::isdigit(rate_string[0])) {
					continue;
				}
				rate = std::stoi(rate_string);

				strong_players.push_back({ name, rate });
			}
		}

		if (strong_players.empty()) {
			return false;
		}

		return true;
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
	o[kBookCsaFolder] << Option("");
	o[kBookTanukiColiseumLogFolder] << Option("");
	o[kBookMinimumWinningPercentage] << Option(0, 0, 100);
	o[kBookBlackMinimumValue] << Option(-VALUE_MATE, -VALUE_MATE, VALUE_MATE);
	o[kBookWhiteMinimumValue] << Option(-VALUE_MATE, -VALUE_MATE, VALUE_MATE);
	o[kBookMinimumCount] << Option(0, 0, INT_MAX);
	o[kBookMinimumRating] << Option(3300, 0, INT_MAX);
	o[kBookUctUcb1Constant] << Option(std::to_string(std::sqrt(2.0)).c_str());
	o[kBookUctTimeMs] << Option(15 * 60 * 1000, 0, INT_MAX);
	o[kBookUctIncMs] << Option(5 * 1000, 0, INT_MAX);
	o[kBookUctNumMatches] << Option(5 * 1000, 0, INT_MAX);
	o[kBookUctMaxSearchPerPosition] << Option(3, 0, INT_MAX);
	o[kBookUctRecordFile] << Option("record.sqlite");

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
	sync_cout << "|input_book|=" << input_book.get_body().size() << sync_endl;

	MemoryBook output_book;
	output_book_file = "book/" + output_book_file;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book(output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body().size() << sync_endl;

	std::vector<std::string> sfens;
	for (const auto& sfen_and_count : input_book.get_body()) {
		if (!overwrite_existing_positions &&
			output_book.get_body().find(sfen_and_count.first) != output_book.get_body().end()) {
			continue;
		}
		sfens.push_back(sfen_and_count.first);
	}
	int num_sfens = static_cast<int>(sfens.size());
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
// BookInputFileには「;」区切りで定跡データベースの古パースを指定する
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
	sync_cout << "|output_book|=" << output_book.get_body().get_body().size() << sync_endl;

	std::vector<std::string> input_files;
	{
		std::istringstream iss(input_file_list);
		std::string input_file;
		while (std::getline(iss, input_file, ';')) {
			input_files.push_back(input_file);
		}
	}

	for (int input_file_index = 0; input_file_index < static_cast<int>(input_files.size()); ++input_file_index) {
		sync_cout << (input_file_index + 1) << " / " << input_files.size() << sync_endl;

		const auto& input_file = input_files[input_file_index];
		BookMoveSelector input_book;
		sync_cout << "Reading input book file: " << input_file << sync_endl;
		input_book.get_body().read_book(input_file);
		sync_cout << "done..." << sync_endl;
		sync_cout << "|input_book|=" << input_book.get_body().get_body().size() << sync_endl;

		for (const auto& book_type : input_book.get_body().get_body()) {
			const auto& sfen = book_type.first;
			const auto& pos_move_list = book_type.second;

			uint64_t max_move_count = 0;
			for (const auto& pos_move : *pos_move_list) {
				max_move_count = std::max(max_move_count, pos_move.move_count);
			}

			for (const auto& pos_move : *pos_move_list) {
				if (max_move_count > 0 && pos_move.move_count == 0) {
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
	sync_cout << "|input_book|=" << input_book.get_body().size() << sync_endl;

	MemoryBook output_book;
	output_book_file = "book/" + output_book_file;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book(output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body().size() << sync_endl;

	std::vector<SfenAndMove> sfen_and_moves;

	// 未処理の局面をキューに登録する
	for (const auto& input_sfen_and_pos_move_list : input_book.get_body()) {
		const auto& input_sfen = input_sfen_and_pos_move_list.first;

		Position& position = Threads[0]->rootPos;
		StateInfo state_info;
		position.set(input_sfen, &state_info, Threads[0]);

		auto output_book_moves_it = output_book.get_body().find(input_sfen);
		if (output_book_moves_it == output_book.get_body().end()) {
			// 出力定跡データベースに局面が登録されていなかった場合
			for (const auto& input_book_move : *input_sfen_and_pos_move_list.second) {
				sfen_and_moves.push_back({
					input_sfen,
					position.to_move(input_book_move.move),
					position.to_move(input_book_move.ponder)
					});
			}
			continue;
		}

		auto& output_book_moves = *output_book_moves_it->second;
		for (const auto& input_book_move : *input_sfen_and_pos_move_list.second) {
			if (std::find_if(output_book_moves.begin(), output_book_moves.end(),
				[&input_book_move](const auto& x) {
					return input_book_move.move == x.move;
				}) != output_book_moves.end()) {
				continue;
			}

			// 出力定跡データベースに指し手が登録されていなかった場合
			sfen_and_moves.push_back({
				input_sfen,
				position.to_move(input_book_move.move),
				position.to_move(input_book_move.ponder)
				});
		}
	}
	int num_sfen_and_moves = static_cast<int>(sfen_and_moves.size());
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
		ASSERT_LV3(book.get_body().count(sfen));

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
			if (book.get_body().find(pos.sfen()) != book.get_body().end()) {
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
		auto book_moves = book.get_body().find(sfen);
		ASSERT_LV3(book_moves != book.get_body().end());
		for (const auto& book_move : *book_moves->second) {
			if (vmd.value < book_move.value) {
				vmd.value = static_cast<Value>(book_move.value);
				vmd.move = pos.to_move(book_move.move);
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
	sync_cout << "|input_book_file|=" << book.get_body().size() << sync_endl;

	// 平手の局面からたどれる局面について処理する
	// メモをクリアするのを忘れない
	memo.clear();
	auto& pos = Threads[0]->rootPos;
	StateInfo state_info = {};
	pos.set_hirate(&state_info, Threads[0]);
	int counter = 0;
	NegaMax(book, pos, counter);

	// 平手の局面から辿れなかった局面を処理する
	for (const auto& book_entry : book.get_body()) {
		auto& pos = Threads[0]->rootPos;
		StateInfo state_info = {};
		pos.set(book_entry.first, &state_info, Threads[0]);
		NegaMax(book, pos, counter);
	}

	WriteBook(book, "book/" + output_book_file);
	sync_cout << "|output_book|=" << book.get_body().size() << sync_endl;

	return true;
}

namespace {
	// 与えられた局面における、与えられた指し手が定跡データベースに含まれているかどうかを返す。
	// 含まれている場合は、その指し手へのポインターを返す。
	// 含まれていない場合は、nullptrを返す。
	BookMove* IsBookMoveExist(BookMoveSelector& book, Position& position, Move move) {
		auto book_moves = book.get_body().find(position);
		if (book_moves == nullptr) {
			return nullptr;
		}

		auto book_move = std::find_if(book_moves->begin(), book_moves->end(),
			[move](auto& m) {
				return m.move == Move16(move);
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
			bool exist = book.get_body().get_body().find(position.sfen()) != book.get_body().get_body().end();
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
		return static_cast<int>(book_moves->size()) < multi_pv;
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
	sync_cout << "|input_book_file|=" << book.get_body().get_body().size() << sync_endl;

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
	sync_cout << "|input_book|=" << input_book.get_body().size() << sync_endl;

	MemoryBook output_book;
	output_book_file = "book/" + output_book_file;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book(output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body().size() << sync_endl;

	// 対象の局面を読み込む
	sync_cout << "Reading target positions: target_sfens_file=" << target_sfens_file << sync_endl;
	std::vector<std::string> lines;
	std::ifstream ifs(target_sfens_file);
	std::string line;
	while (std::getline(ifs, line)) {
		lines.push_back(line);
	}
	int num_positions = static_cast<int>(lines.size());
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
	sync_cout << "|input_book|=" << input_book.get_body().size() << sync_endl;

	MemoryBook output_book;
	output_book_file = "book/" + output_book_file;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book(output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body().size() << sync_endl;

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

			for (int move_index = winner_offset; move_index < static_cast<int>(moves.size()); move_index += 2) {
				const auto& sfen = moves[move_index].first;
				Move16 best = moves[move_index].second;
				Move16 next = MOVE_NONE;
				if (move_index + 1 < static_cast<int>(moves.size())) {
					next = moves[move_index + 1].second;
				}

				output_book.insert(sfen, BookMove(best, next, 0, 0, 1));
			}
		}
	}

	WriteBook(output_book, output_book_file);

	return true;
}

bool Tanuki::Create18Book() {
	std::string csa_folder = Options[kBookCsaFolder];
	std::string output_book_file = Options[kBookOutputFile];
	int minimum_rating = static_cast<int>(Options[kBookMinimumRating]);

	MemoryBook output_book;
	output_book_file = "book/" + output_book_file;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book(output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body().size() << sync_endl;

	std::vector<Player> strong_players;
	if (!ReadStrongPlayers(strong_players)) {
		sync_cout << "Failed to read the player list." << sync_endl;
		return false;
	}

	int num_records = 0;
	for (const std::filesystem::directory_entry& entry :
		std::filesystem::recursive_directory_iterator(csa_folder)) {
		auto& file_path = entry.path();
		if (file_path.extension() != ".csa") {
			// 拡張子が .csa でなかったらスキップする。
			continue;
		}

		if (std::count_if(strong_players.begin(), strong_players.end(),
			[&file_path, minimum_rating](const auto& strong_player) {
				// レーティングが低いソフトの棋譜は使用しない。
				if (strong_player.rate < minimum_rating) {
					return false;
				}

				return file_path.string().find("+" + strong_player.name + "+") != std::string::npos;
			}) != 2) {
			// 強いプレイヤー同士の対局でなかったらスキップする。
			continue;
		}

		if (++num_records % 1000 == 0) {
			sync_cout << num_records << sync_endl;
		}

		auto& pos = Threads[0]->rootPos;
		StateInfo state_info[512];
		pos.set_hirate(&state_info[0], Threads[0]);

		FILE* file = std::fopen(file_path.string().c_str(), "r");

		if (file == nullptr) {
			std::cout << "!!! Failed to open the input file: filepath=" << file_path << std::endl;
			continue;
		}

		bool toryo = false;
		std::string black_player_name;
		std::string white_player_name;
		std::vector<Move> moves;
		int winner_offset = 0;
		char buffer[1024];
		while (std::fgets(buffer, sizeof(buffer) - 1, file)) {
			std::string line = buffer;
			// std::fgets()の出力は行末の改行を含むため、ここで削除する。
			while (!line.empty() && std::isspace(line.back())) {
				line.pop_back();
			}

			auto offset = line.find(',');
			if (offset != std::string::npos) {
				// 将棋所の出力するCSAの指し手の末尾に",T1"などとつくため
				// ","以降を削除する
				line = line.substr(0, offset);
			}

			if (line.find("N+") == 0) {
				black_player_name = line.substr(2);
			}
			else if (line.find("N-") == 0) {
				white_player_name = line.substr(2);
			}
			else if (line.size() == 7 && (line[0] == '+' || line[0] == '-')) {
				Move move = CSA::to_move(pos, line.substr(1));

				if (!pos.pseudo_legal(move) || !pos.legal(move)) {
					std::cout << "!!! Found an illegal move." << std::endl;
					break;
				}

				pos.do_move(move, state_info[pos.game_ply()]);

				moves.push_back(move);
			}
			else if (line.find(black_player_name + " win") != std::string::npos) {
				winner_offset = 0;
			}
			else if (line.find(white_player_name + " win") != std::string::npos) {
				winner_offset = 1;
			}

			if (line.find("toryo") != std::string::npos) {
				toryo = true;
			}
		}

		std::fclose(file);
		file = nullptr;

		// 投了以外の棋譜はスキップする
		if (!toryo) {
			continue;
		}

		pos.set_hirate(&state_info[0], Threads[0]);

		for (int play = 0; play < static_cast<int>(moves.size()); ++play) {
			Move move = moves[play];
			if (play % 2 == winner_offset) {
				Move ponder = (play + 1 < static_cast<int>(moves.size())) ? moves[play + 1] : Move::MOVE_NONE;
				output_book.insert(pos.sfen(), Book::BookMove(move, ponder, 0, 0, 1));
			}
			pos.do_move(move, state_info[pos.game_ply()]);
		}
	}

	WriteBook(output_book, output_book_file);

	return true;
}

namespace {
	struct InternalBookMove {
		Move16 move = Move::MOVE_NONE;   // この局面での指し手
		Move16 ponder = Move::MOVE_NONE; // その指し手を指したときの予想される相手の指し手(指し手が無いときはnoneと書くことになっているので、このとMOVE_NONEになっている)
		// この手を指した場合の勝利回数
		int num_win = 0;
		// この手を指した場合の敗北回数
		int num_lose = 0;
		// 評価値が付いていた指し手について、評価値の総和
		s64 sum_values = 0;
		// 評価値が付いていた指し手の数
		int num_values = 0;
	};
	using InternalBook = std::map<std::string, std::map<u16 /* Move16 */, InternalBookMove>>;

	bool ReadCsaFile(const std::string& file_path, std::vector<Move>& moves, bool& toryo, int& winner_offset) {
		auto& pos = Threads[0]->rootPos;
		StateInfo state_info[512];
		pos.set_hirate(&state_info[0], Threads[0]);

		FILE* file = std::fopen(file_path.c_str(), "r");

		if (file == nullptr) {
			std::cout << "!!! Failed to open the input file: filepath=" << file_path << std::endl;
			return false;
		}

		std::string black_player_name;
		std::string white_player_name;
		char buffer[1024];
		while (std::fgets(buffer, sizeof(buffer) - 1, file)) {
			std::string line = buffer;
			// std::fgets()の出力は行末の改行を含むため、ここで削除する。
			while (!line.empty() && std::isspace(line.back())) {
				line.pop_back();
			}

			auto offset = line.find(',');
			if (offset != std::string::npos) {
				// 将棋所の出力するCSAの指し手の末尾に",T1"などとつくため
				// ","以降を削除する
				line = line.substr(0, offset);
			}

			if (line.find("N+") == 0) {
				black_player_name = line.substr(2);
			}
			else if (line.find("N-") == 0) {
				white_player_name = line.substr(2);
			}
			else if (line.size() == 7 && (line[0] == '+' || line[0] == '-')) {
				Move move = CSA::to_move(pos, line.substr(1));

				if (!pos.pseudo_legal(move) || !pos.legal(move)) {
					std::cout << "!!! Found an illegal move." << std::endl;
					break;
				}

				pos.do_move(move, state_info[pos.game_ply()]);

				moves.push_back(move);
			}
			else if (line.find(black_player_name + " win") != std::string::npos) {
				winner_offset = 0;
			}
			else if (line.find(white_player_name + " win") != std::string::npos) {
				winner_offset = 1;
			}

			if (line.find("toryo") != std::string::npos) {
				toryo = true;
			}
		}

		std::fclose(file);
		file = nullptr;

		return true;
	}

	void ParseFloodgateCsaFiles(const std::string& csa_folder_path,
		const std::vector<Player>& strong_players, int minimum_rating,
		InternalBook& internal_book) {
		int num_records = 0;
		for (const std::filesystem::directory_entry& entry :
			std::filesystem::recursive_directory_iterator(csa_folder_path)) {
			auto& file_path = entry.path();
			if (file_path.extension() != ".csa") {
				// 拡張子が .csa でなかったらスキップする。
				continue;
			}

			if (std::count_if(strong_players.begin(), strong_players.end(),
				[&file_path, minimum_rating](const auto& strong_player) {
					// レーティングが低いソフトの棋譜は使用しない。
					if (strong_player.rate < minimum_rating) {
						return false;
					}

					return file_path.string().find("+" + strong_player.name + "+") != std::string::npos;
				}) != 2) {
				// 強いプレイヤー同士の対局でなかったらスキップする。
				continue;
			}

			if (++num_records % 1000 == 0) {
				sync_cout << num_records << sync_endl;
			}

			bool toryo = false;
			std::vector<Move> moves;
			int winner_offset = 0;
			if (!ReadCsaFile(file_path.string(), moves, toryo, winner_offset)) {
				sync_cout << "Failed to read a csa file. file_path" << file_path << sync_endl;
				continue;
			}

			// 投了以外の棋譜はスキップする
			if (!toryo) {
				continue;
			}

			auto& pos = Threads[0]->rootPos;
			StateInfo state_info[512];
			pos.set_hirate(&state_info[0], Threads[0]);

			for (int play = 0; play < static_cast<int>(moves.size()); ++play) {
				auto move = moves[play];
				if (!pos.pseudo_legal(move) || !pos.legal(move)) {
					std::cout << "Illegal move. sfen=" << pos.sfen() << " move=" << move << std::endl;
					break;
				}

				auto& internal_book_move = internal_book[pos.sfen()][static_cast<u16>(move)];
				internal_book_move.move = move;
				internal_book_move.ponder = (play + 1 < static_cast<int>(moves.size())) ? moves[play + 1] : Move::MOVE_NONE;
				if (play % 2 == winner_offset) {
					++internal_book_move.num_win;
				}
				else {
					++internal_book_move.num_lose;
				}

				pos.do_move(move, state_info[pos.game_ply()]);
			}
		}
	}

	int GetGameIdsAndWinners(sqlite3* database, std::vector<std::pair<int, int>>& game_ids_and_winners) {
		sync_cout << "GetGameIdsAndWinners()" << sync_endl;

		sqlite3_stmt* stmt = NULL;

		// ゲームIDと勝者の取得。
		int result = sqlite3_prepare(database, "SELECT id, winner FROM game ORDER BY id", -1, &stmt, nullptr);
		if (result != SQLITE_OK) {
			sync_cout << "Faile to call sqlite3_prepare(). errmsg=" << sqlite3_errmsg(database) << sync_endl;
			return result;
		}

		if (stmt == nullptr) {
			// コメント等、有効なSQLステートメントでないと、戻り値はOKだがstmはNULLになる。
			return -1;
		}

		int num_columns = sqlite3_column_count(stmt);
		int column_id = -1;
		int column_winner = -1;
		for (int column = 0; column < num_columns; ++column) {
			std::string column_name = sqlite3_column_name(stmt, column);
			if (column_name == "id") {
				column_id = column;
			}
			else if (column_name == "winner") {
				column_winner = column;
			}
			else {
				sync_cout << "Invalid column name. column_name=" << column_name << sync_endl;
				return -1;
			}
		}

		while ((result = sqlite3_step(stmt)) == SQLITE_ROW) {
			int id = -1;
			int winner = -1;
			for (int column = 0; column < num_columns; ++column) {
				if (column_id == column) {
					id = sqlite3_column_int(stmt, column);
				}
				else if (column_winner == column) {
					winner = sqlite3_column_int(stmt, column);
				}
				else {
					sync_cout << "Unknown column. column=" << column << sync_endl;
				}
			}

			if (id == -1) {
				sync_cout << "id is not contained." << sync_endl;
				continue;
			}

			if (winner == -1) {
				sync_cout << "winner is not contained." << sync_endl;
				continue;
			}

			game_ids_and_winners.emplace_back(id, winner);
		}

		sqlite3_finalize(stmt);
		stmt = nullptr;

		if (result == SQLITE_ERROR) {
			sync_cout << "Faile to call sqlite3_step(). errmsg=" << sqlite3_errmsg(database) << sync_endl;
			return result;
		}

		return SQLITE_OK;
	}

	u16 to_move16(const std::string& str) {
		if (str == "none") {
			return static_cast<u16>(Move::MOVE_NONE);
		}
		return USI::to_move16(str).to_u16();
	}

	struct InternalMove {
		u16 best;
		u16 next;
		int value;
		int book;
		int depth;
	};

	int GetRecord(sqlite3* database, std::map<int, std::map<int, InternalMove>>& internal_moves) {
		sync_cout << "GetRecord()" << sync_endl;

		sqlite3_stmt* stmt = NULL;

		int result = sqlite3_prepare(database, "SELECT game_id, play, best, next, value, book, depth FROM move", -1, &stmt, nullptr);
		if (result != SQLITE_OK) {
			sync_cout << "Faile to call sqlite3_prepare(). errmsg=" << sqlite3_errmsg(database) << sync_endl;
			return result;
		}

		if (stmt == nullptr) {
			// コメント等、有効なSQLステートメントでないと、戻り値はOKだがstmはNULLになる。
			return -1;
		}

		int column_game_id = -1;
		int column_play = -1;
		int column_best = -1;
		int column_next = -1;
		int column_value = -1;
		int column_book = -1;
		int column_depth = -1;
		int num_columns = sqlite3_column_count(stmt);
		for (int column = 0; column < num_columns; ++column) {
			std::string column_name = sqlite3_column_name(stmt, column);
			if (column_name == "game_id") {
				column_game_id = column;
			}
			else if (column_name == "play") {
				column_play = column;
			}
			else if (column_name == "best") {
				column_best = column;
			}
			else if (column_name == "next") {
				column_next = column;
			}
			else if (column_name == "value") {
				column_value = column;
			}
			else if (column_name == "book") {
				column_book = column;
			}
			else if (column_name == "depth") {
				column_depth = column;
			}
			else {
				sync_cout << "Invalid column name. column_name=" << column_name << sync_endl;
			}
		}

		// カラム数の取得
		while ((result = sqlite3_step(stmt)) == SQLITE_ROW) {
			int game_id = -1;
			int play = -1;
			std::string best;
			std::string next;
			int value = Value::VALUE_NONE;
			int book = -1;
			int depth = 0;
			for (int column = 0; column < num_columns; ++column) {
				if (column_game_id == column) {
					game_id = sqlite3_column_int(stmt, column);
				}
				else if (column_play == column) {
					play = sqlite3_column_int(stmt, column);
				}
				else if (column_best == column) {
					best = reinterpret_cast<const char*>(sqlite3_column_text(stmt, column));
				}
				else if (column_next == column) {
					next = reinterpret_cast<const char*>(sqlite3_column_text(stmt, column));
				}
				else if (column_value == column) {
					value = sqlite3_column_int(stmt, column);
				}
				else if (column_book == column) {
					book = sqlite3_column_int(stmt, column);
				}
				else if (column_depth == column) {
					depth = sqlite3_column_int(stmt, column);
				}
				else {
					sync_cout << "Invalid column. column=" << column << sync_endl;
				}
			}

			if (game_id == -1) {
				sync_cout << "game_id is not recorded." << sync_endl;
				continue;
			}

			if (play == -1) {
				sync_cout << "play is not recorded." << sync_endl;
				continue;
			}

			if (best.empty()) {
				sync_cout << "best is not recorded." << sync_endl;
				continue;
			}

			if (next.empty()) {
				sync_cout << "next is not recorded." << sync_endl;
				continue;
			}

			if (value == Value::VALUE_NONE) {
				sync_cout << "value is not recorded." << sync_endl;
				continue;
			}

			if (book == -1) {
				sync_cout << "book is not recorded." << sync_endl;
				continue;
			}

			internal_moves[game_id][play] = { to_move16(best), to_move16(next), value, book, depth };
		}

		sqlite3_finalize(stmt);
		stmt = nullptr;

		if (result == SQLITE_ERROR) {
			sync_cout << "Faile to call sqlite3_step(). errmsg=" << sqlite3_errmsg(database) << sync_endl;
			return result;
		}

		return SQLITE_OK;
	}

	void ParseTanukiColiseumResultFiles(const std::string& tanuki_coliseum_folder_path, InternalBook& internal_book) {
		if (!std::filesystem::is_directory(tanuki_coliseum_folder_path)) {
			sync_cout << "TanukiColiseum folder was not found. tanuki_coliseum_folder_path=" << tanuki_coliseum_folder_path << sync_endl;
			return;
		}

		std::vector<StateInfo> state_infos(512);
		int num_records = 0;
		for (const std::filesystem::directory_entry& entry :
			std::filesystem::recursive_directory_iterator(tanuki_coliseum_folder_path)) {
			auto& file_path = entry.path();
			if (file_path.extension() != ".sqlite3") {
				// 拡張子が .csa でなかったらスキップする。
				continue;
			}

			// C++からSQLite3を使ってみる。 - seraphyの日記 https://seraphy.hatenablog.com/entry/20061031/p1
			sqlite3* database = nullptr;
			int result = 0;

			result = sqlite3_open(file_path.string().c_str(), &database);
			if (result != SQLITE_OK) {
				sqlite3_close(database);
				database = nullptr;

				sync_cout << "Failed to open an sqlite file. file_path=" << file_path << sync_endl;
				continue;
			}

			std::vector<std::pair<int, int>> game_ids_and_winners;
			GetGameIdsAndWinners(database, game_ids_and_winners);

			std::map<int, std::map<int, InternalMove>> internal_moves;
			GetRecord(database, internal_moves);

			sqlite3_close(database);
			database = nullptr;

			sync_cout << "Constructing an internal book." << sync_endl;

			for (auto& [game_id, winner] : game_ids_and_winners) {
				if (winner > 1) {
					// 引き分けは取り除く
					continue;
				}

				auto& position = Threads[0]->rootPos;
				position.set_hirate(&state_infos[0], Threads[0]);

				for (auto& [play, move] : internal_moves[game_id]) {
					auto move32 = position.to_move(move.best);
					if (!position.pseudo_legal(move32) || !position.legal(move32)) {
						sync_cout << "Illegal move. sfen=" << position.sfen() << " move=" << move32 << sync_endl;
						break;
					}

					auto& internal_book_move = internal_book[position.sfen()][move.best];
					internal_book_move.move = move.best;
					internal_book_move.ponder = move.next;
					if (play % 2 == winner) {
						++internal_book_move.num_win;
					}
					else {
						++internal_book_move.num_lose;
					}

					if (!move.book) {
						++internal_book_move.num_values;
						internal_book_move.sum_values += move.value;
					}

					position.do_move(move32, state_infos[position.game_ply()]);
				}
			}
		}
	}

	int WriteGameIdsAndWinners(sqlite3_stmt* stmt, int game_id, int winner) {
		int result = 0;

		result = sqlite3_bind_int(stmt, 1, game_id);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to bind a value to a statement."
				<< " i=" << 1
				<< " iValue=" << game_id
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_bind_int(stmt, 2, winner);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to bind a value to a statement."
				<< " i=" << 2
				<< " iValue=" << winner
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_step(stmt);
		if (result != SQLITE_DONE) {
			sync_cout << "Failed to execute a statement."
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_reset(stmt);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to reset a statement."
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_clear_bindings(stmt);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to clear bindings."
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		return SQLITE_OK;
	}

	int WriteMove(sqlite3_stmt* stmt, int game_id, int play, const InternalMove& internal_move) {
		int result = 0;

		result = sqlite3_bind_int(stmt, 1, game_id);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to bind a value to a statement."
				<< " i=" << 1
				<< " iValue=" << game_id
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_bind_int(stmt, 2, play);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to bind a value to a statement."
				<< " i=" << 2
				<< " iValue=" << play
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_bind_text(stmt, 3, to_usi_string(internal_move.best).c_str(), -1, SQLITE_TRANSIENT);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to bind a value to a statement."
				<< " i=" << 3
				<< " zData=" << to_usi_string(internal_move.best)
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_bind_text(stmt, 4, to_usi_string(internal_move.next).c_str(), -1, SQLITE_TRANSIENT);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to bind a value to a statement."
				<< " i=" << 4
				<< " zData=" << to_usi_string(internal_move.next)
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_bind_int(stmt, 5, internal_move.value);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to bind a value to a statement."
				<< " i=" << 5
				<< " iValue=" << internal_move.value
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_bind_int(stmt, 6, internal_move.depth);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to bind a value to a statement."
				<< " i=" << 6
				<< " iValue=" << internal_move.depth
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_bind_int(stmt, 7, internal_move.book);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to bind a value to a statement."
				<< " i=" << 7
				<< " iValue=" << internal_move.book
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_step(stmt);
		if (result != SQLITE_DONE) {
			sync_cout << "Failed to execute a statement."
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_reset(stmt);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to reset a statement."
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = sqlite3_clear_bindings(stmt);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to clear bindings."
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		return SQLITE_OK;
	}

	int WriteRecordToSQLiteBody(sqlite3* database, int winner, const std::vector<InternalMove>& internal_moves) {
		char* error_message = nullptr;
		int result = 0;
		int finalize_result = 0;
		sqlite3_stmt* stmt = NULL;

		// トランザクションを開始する
		result = sqlite3_exec(database, "BEGIN", 0, 0, &error_message);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to execute BEGIN."
				<< " result=" << result
				<< " error_message=" << error_message
				<< sync_endl;
			return result;
		}

		// テーブルを作成する
		result = sqlite3_exec(database,
			"CREATE TABLE IF NOT EXISTS game("
			"	id INTEGER PRIMARY KEY ASC,"
			"	winner INTEGER"
			");", 0, 0, &error_message);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to create the game table."
				<< " result=" << result
				<< " error_message=" << error_message
				<< sync_endl;
			return result;
		}

		result = sqlite3_exec(database,
			"CREATE TABLE IF NOT EXISTS move ("
			"	game_id INTEGER,"
			"	play INTEGER,"
			"	best TEXT,"
			"	next TEXT,"
			"	value INTEGER,"
			"	depth INTEGER,"
			"	book INTEGER"
			");", 0, 0, &error_message);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to create the game table."
				<< " result=" << result
				<< " error_message=" << error_message
				<< sync_endl;
			return result;
		}

		// インデックスの作成
		result = sqlite3_exec(database,
			"CREATE INDEX IF NOT EXISTS move_game_id_index ON move ("
			"	game_id"
			");", 0, 0, &error_message);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to create the game table."
				<< " result=" << result
				<< " error_message=" << error_message
				<< sync_endl;
			return result;
		}

		// すでに記録されている対局の結果を読み込む
		std::vector<std::pair<int, int>> game_ids_and_winners;
		result = GetGameIdsAndWinners(database, game_ids_and_winners);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to get ids and winners."
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		int last_game_id = 0;
		for (auto& [game_id, winner] : game_ids_and_winners) {
			last_game_id = std::max(last_game_id, game_id);
		}

		int game_id = last_game_id + 1;

		// 対局IDと対局結果の書き込み
		result = sqlite3_prepare(database,
			"INSERT INTO game(id, winner)"
			"VALUES(?, ?)", -1, &stmt, nullptr);
		if (result != SQLITE_OK || stmt == nullptr) {
			sync_cout << "Failed to prepare a statement."
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		result = WriteGameIdsAndWinners(stmt, game_id, winner);

		finalize_result = sqlite3_finalize(stmt);
		stmt = nullptr;

		if (result != SQLITE_OK) {
			// エラーメッセージは表示済み
			return result;
		}

		if (finalize_result != SQLITE_OK) {
			sync_cout << "Failed to finalize a statement."
				<< " finalize_result=" << finalize_result
				<< sync_endl;
			return result;
		}

		// 棋譜の書き込み
		for (int play = 0; play < static_cast<int>(internal_moves.size()); ++play) {
			result = sqlite3_prepare(database,
				"INSERT INTO move(game_id, play, best, next, value, depth, book)"
				"VALUES(?, ?, ?, ?, ?, ?, ?)", -1, &stmt, nullptr);
			if (result != SQLITE_OK || stmt == nullptr) {
				sync_cout << "Failed to prepare a statement."
					<< " result=" << result
					<< sync_endl;
				return result;
			}

			result = WriteMove(stmt, game_id, play, internal_moves[play]);

			finalize_result = sqlite3_finalize(stmt);
			stmt = nullptr;

			if (result != SQLITE_OK) {
				// エラーメッセージは表示済み
				return result;
			}

			if (finalize_result != SQLITE_OK) {
				sync_cout << "Failed to finalize a statement."
					<< " finalize_result=" << finalize_result
					<< sync_endl;
				return result;
			}
		}

		// トランザクションを終了する
		result = sqlite3_exec(database, "COMMIT", 0, 0, &error_message);
		if (result != SQLITE_OK) {
			sync_cout << "Failed to execute COMMIT."
				<< " result=" << result
				<< " error_message=" << error_message
				<< sync_endl;
			return result;
		}

		result = sqlite3_close(database);
		database = nullptr;
		if (result != SQLITE_OK) {
			sync_cout << "Failed to close a databse."
				<< " result=" << result
				<< sync_endl;
			return result;
		}

		return result;
	}

	bool WriteRecordToSQLite(const std::string& record_file, int winner, const std::vector<InternalMove>& internal_moves) {
		// SQLiteデータベースへの棋譜の保存
		sqlite3* database = nullptr;
		int result = 0;

		result = sqlite3_open(record_file.c_str(), &database);
		if (result == SQLITE_OK) {
			result = WriteRecordToSQLiteBody(database, winner, internal_moves);
		} else {
			sync_cout << "Failed to open an sqlite file."
				<< " record_file=" << record_file
				<< sync_endl;
		}

		sqlite3_close(database);
		database = nullptr;
		return result == SQLITE_OK;
	}

	using BadMove = std::pair<std::string, std::string>;
	static const std::vector<BadMove> BadMoves = {
		{"lnsgk1snl/1r4gb1/p1pppp1pp/1p4p2/7P1/2P6/PP1PPPP1P/1BG4R1/LNS1KGSNL w - 8", "8d8e"},
		{"lnsgkgsnl/1r5b1/p1pppp1pp/1p4p2/7P1/2P6/PP1PPPP1P/1B5R1/LNSGKGSNL w - 6", "8d8e"},
		{"lnsgkgsnl/1r5b1/ppppppppp/9/9/7P1/PPPPPPP1P/1B5R1/LNSGKGSNL w - 2", "4a3b"},
		{"ln5nl/1r2gkg2/3ppp1p1/p2s1sp1p/1pp4P1/2PPSPP2/PPS1P1N1P/2GK1G3/LN5RL b Bb 35", "7f7e"},
		{"ln5nl/4gkg2/4ppsp1/p2p2p1p/5P1P1/1rPPS1P2/P3P1N1P/2GK1G3/LN5RL b BSPbs2p 49", "P*8g"},
		{"ln5nl/1r2gkg2/3ppp1p1/p4sp1p/1ps4P1/3PSPP2/PPS1P1N1P/2GK1G3/LN5RL b BPbp 37", "4f4e"},
	};

	void RemoveBadMove(InternalBook& internal_book) {
		sync_cout << "RemoveBadMove()" << sync_endl;
		for (auto& [sfen, move_string] : BadMoves) {
			auto it = internal_book.find(sfen);
			if (it == internal_book.end()) {
				sync_cout << "Falied to remove a bad move. Position was not found. sfen=" << sfen << " move=" << move_string << sync_endl;
				continue;
			}

			u16 move16 = USI::to_move16(move_string).to_u16();
			auto jt = it->second.find(move16);
			if (jt == it->second.end()) {
				sync_cout << "Falied to remove a bad move. Move was not found. sfen=" << sfen << " move=" << move_string << sync_endl;
				continue;
			}

			it->second.erase(jt);

			if (it->second.empty()) {
				internal_book.erase(it);
			}

			sync_cout << "Removed a bad move. sfen=" << sfen << " move=" << move_string << sync_endl;
		}
	}

	void RemoveBadMove2(const std::string& csa_folder, InternalBook& internal_book) {
		sync_cout << "RemoveBadMove2()" << sync_endl;
		std::ifstream ifs("bad_moves.txt");
		std::string url;
		int target_play;
		while (ifs >> url >> target_play) {
			int offset = static_cast<int>(url.find_last_of("/"));
			std::string file_name = url.substr(offset + 1);
			std::string file_path = csa_folder + "\\wdoor2021\\2021\\" + file_name;

			std::vector<Move> moves;
			bool toryo = false;
			int winner_offset = 0;
			if (!ReadCsaFile(file_path, moves, toryo, winner_offset)) {
				sync_cout << "Failed to read a csa file. file_path" << file_path << sync_endl;
				continue;
			}

			auto& pos = Threads[0]->rootPos;
			std::vector<StateInfo> state_info(512);
			pos.set_hirate(&state_info[0], Threads[0]);
			for (int play = 0; play + 1 < target_play; ++play) {
				pos.do_move(moves[play], state_info[pos.game_ply()]);
			}

			u16 move16 = moves[target_play - 1];
			std::string move_string = USI::move({ moves[target_play - 1] });
			std::string sfen = pos.sfen();

			auto it = internal_book.find(sfen);
			if (it == internal_book.end()) {
				sync_cout << "Falied to remove a bad move. Position was not found. sfen=" << sfen << " move=" << move_string << sync_endl;
				continue;
			}

			auto jt = it->second.find(move16);
			if (jt == it->second.end()) {
				sync_cout << "Falied to remove a bad move. Move was not found. sfen=" << sfen << " move=" << move_string << sync_endl;
				continue;
			}

			it->second.erase(jt);

			if (it->second.empty()) {
				internal_book.erase(it);
			}

			sync_cout << "Removed a bad move. sfen=" << sfen << " move=" << move_string << sync_endl;
		}
	}

	static const std::vector<BadMove> GoodMoves = {
		{"lr5nl/3gk1g2/2n1ppsp1/p1pps3p/1P4SP1/P1PP4P/2SGPP3/2G4R1/LNK4NL w B3Pb 43", "8a8e"},
	};

	void AddGoodMove(InternalBook& internal_book) {
		sync_cout << "AddGoodMove()" << sync_endl;
		for (auto& [sfen, move_string] : GoodMoves) {
			Move16 move16 = USI::to_move16(move_string);

			auto& internal_book_move = internal_book[sfen][move16.to_u16()];
			internal_book_move.move = move16;
			internal_book_move.ponder = Move::MOVE_NONE;
			++internal_book_move.num_win;
		}
	}

	void ReadInternalBook(const std::string& file_path, InternalBook& internal_book) {
		sync_cout << "ReadInternalBook(): file_path=" << file_path << sync_endl;
		int counter = 0;
		std::ifstream ifs(file_path);
		std::string sfen;
		while (std::getline(ifs, sfen)) {
			if (++counter % 100000 == 0) {
				sync_cout << counter << sync_endl;
			}

			int num_book_moves;
			ifs >> num_book_moves;

			for (int book_move_index = 0; book_move_index < num_book_moves; ++book_move_index) {
				std::string move_string;
				std::string ponder_string;
				int num_win;
				int num_lose;
				s64 sum_values;
				int num_values;
				ifs >> move_string >> ponder_string >> num_win >> num_lose >> sum_values >> num_values;
				u16 move16 = USI::to_move16(move_string).to_u16();
				u16 ponder16 = ponder_string == "none" ? static_cast<u16>(Move::MOVE_NONE) : USI::to_move16(ponder_string).to_u16();
				internal_book[sfen][move16] = { move16, ponder16, num_win, num_lose, sum_values, num_values };
			}

			// 末尾の改行を読む
			std::string _;
			std::getline(ifs, _);
		}
		sync_cout << "done." << sync_endl;
	}

	void WriteInternalBook(const std::string& file_path, const InternalBook& internal_book) {
		sync_cout << "WriteInternalBook(): file_path=" << file_path << sync_endl;
		int counter = 0;
		std::ofstream ofs(file_path);
		for (const auto& [sfen, move16_to_book_move] : internal_book) {
			if (++counter % 100000 == 0) {
				sync_cout << counter << "/" << internal_book.size() << sync_endl;
			}

			ofs << sfen << std::endl;
			ofs << move16_to_book_move.size() << std::endl;
			for (const auto& [move16, book_move] : move16_to_book_move) {
				ofs
					<< to_usi_string(book_move.move) << " "
					<< to_usi_string(book_move.ponder) << " "
					<< book_move.num_win << " "
					<< book_move.num_lose << " "
					<< book_move.sum_values << " "
					<< book_move.num_values << std::endl;
			}
		}
		sync_cout << "done." << sync_endl;
	}
}

bool Tanuki::CreateTayayanBook() {
	std::string csa_folder = Options[kBookCsaFolder];
	std::string tanuki_coliseum_log_folder = Options[kBookTanukiColiseumLogFolder];
	std::string output_book_file = Options[kBookOutputFile];
	int minimum_winning_percentage = static_cast<int>(Options[kBookMinimumWinningPercentage]);
	int minimum_rating = static_cast<int>(Options[kBookMinimumRating]);

	MemoryBook output_book;
	output_book_file = "book/" + output_book_file;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book(output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body().size() << sync_endl;

	std::vector<Player> strong_players;
	if (!ReadStrongPlayers(strong_players)) {
		sync_cout << "Failed to read the player list." << sync_endl;
		return false;
	}

	InternalBook internal_book;
	ParseFloodgateCsaFiles(csa_folder, strong_players, minimum_rating, internal_book);
	ParseTanukiColiseumResultFiles(tanuki_coliseum_log_folder, internal_book);

	for (auto& [sfen, best16_to_book_move] : internal_book) {
		for (auto& [best16, book_move] : best16_to_book_move) {
			auto move = book_move.move;
			auto ponder = book_move.ponder;
			int value = book_move.num_values ? static_cast<int>(book_move.sum_values / book_move.num_values) : 0;
			int count = book_move.num_win + book_move.num_lose;

			// book_move.num_win / count < minimum_winning_percentage / 100
			if (book_move.num_win * 100 < minimum_winning_percentage * count) {
				continue;
			}

			auto& position = Threads[0]->rootPos;
			StateInfo state_info;
			position.set(sfen, &state_info, Threads[0]);
			auto move32 = position.to_move(move);
			if (!position.pseudo_legal(move32) || !position.legal(move32)) {
				sync_cout << "Illegal move. sfen=" << position.sfen() << " move=" << move32 << sync_endl;
				continue;
			}

			output_book.insert(sfen, Book::BookMove(move, ponder, value, 0, count));
		}
	}

	WriteBook(output_book, output_book_file);

	return true;
}

bool Tanuki::CreateTayayanBook2() {
	std::string csa_folder = Options[kBookCsaFolder];
	std::string tanuki_coliseum_log_folder = Options[kBookTanukiColiseumLogFolder];
	std::string output_book_file = Options[kBookOutputFile];
	int minimum_winning_percentage = static_cast<int>(Options[kBookMinimumWinningPercentage]);
	int black_minimum_value = static_cast<int>(Options[kBookBlackMinimumValue]);
	int white_minimum_value = static_cast<int>(Options[kBookWhiteMinimumValue]);
	int minimum_count = static_cast<int>(Options[kBookMinimumCount]);
	int minimum_rating = static_cast<int>(Options[kBookMinimumRating]);

	MemoryBook output_book;
	output_book_file = "book/" + output_book_file;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book(output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body().size() << sync_endl;

	std::vector<Player> strong_players;
	if (!ReadStrongPlayers(strong_players)) {
		sync_cout << "Failed to read the player list." << sync_endl;
		return false;
	}

	InternalBook internal_book;
	ParseFloodgateCsaFiles(csa_folder, strong_players, minimum_rating, internal_book);
	ParseTanukiColiseumResultFiles(tanuki_coliseum_log_folder, internal_book);
	RemoveBadMove(internal_book);
	RemoveBadMove2(csa_folder, internal_book);

	for (auto& [sfen, best16_to_book_move] : internal_book) {
		for (auto& [best16, book_move] : best16_to_book_move) {
			auto move = book_move.move;
			auto ponder = book_move.ponder;
			int value = book_move.num_values ? static_cast<int>(book_move.sum_values / book_move.num_values) : 0;
			int count = book_move.num_win + book_move.num_lose;
			auto color = (sfen.find(" b ") != std::string::npos ? BLACK : WHITE);

			// 勝率が一定値以下の指し手を削除する。
			// book_move.num_win / count < minimum_winning_percentage / 100
			if (book_move.num_win * 100 < minimum_winning_percentage * count) {
				continue;
			}

			// 評価値が一定値以下の指し手を削除する。
			// TODO(hnoda): book_move.num_values に修正する。
			if (count > 0 && ((color == BLACK && value < black_minimum_value) || (color == WHITE && value < white_minimum_value))) {
				continue;
			}

			// 出現回数が一定値以下の指し手を削除する。
			if (count < minimum_count) {
				continue;
			}

			auto& position = Threads[0]->rootPos;
			StateInfo state_info;
			position.set(sfen, &state_info, Threads[0]);
			auto move32 = position.to_move(move);
			if (!position.pseudo_legal(move32) || !position.legal(move32)) {
				sync_cout << "Illegal move. sfen=" << position.sfen() << " move=" << move32 << sync_endl;
				continue;
			}

			output_book.insert(sfen, Book::BookMove(move, ponder, value, 0, count));
		}
	}

	WriteBook(output_book, output_book_file);

	return true;
}

bool Tanuki::CreateInternalBookFromFloodgateRecords() {
	std::string csa_folder = Options[kBookCsaFolder];
	std::string output_book_file = Options[kBookOutputFile];
	int minimum_rating = static_cast<int>(Options[kBookMinimumRating]);

	std::vector<Player> strong_players;
	if (!ReadStrongPlayers(strong_players)) {
		sync_cout << "Failed to read the player list." << sync_endl;
		return false;
	}

	InternalBook internal_book;
	ParseFloodgateCsaFiles(csa_folder, strong_players, minimum_rating, internal_book);
	WriteInternalBook("book\\" + output_book_file, internal_book);

	return true;
}

bool Tanuki::CreateUctBook() {
	std::string output_book_file = Options[kBookOutputFile];
	double ucb1_constant = std::stof(Options[kBookUctUcb1Constant]);
	int time_ms = static_cast<int>(Options[kBookUctTimeMs]);
	int inc_ms = static_cast<int>(Options[kBookUctIncMs]);
	int num_matches = static_cast<int>(Options[kBookUctNumMatches]);
	int max_moves_to_draw = static_cast<int>(Options["MaxMovesToDraw"]);
	int max_search_per_position = static_cast<int>(Options[kBookUctMaxSearchPerPosition]);
	int resign_value = static_cast<int>(Options["ResignValue"]);
	std::string record_file = Options[kBookUctRecordFile];
	ASSERT_LV3(max_moves_to_draw > 0);

	sync_cout << "output_book_file=" << output_book_file << sync_endl;
	sync_cout << "ucb1_constant=" << ucb1_constant << sync_endl;
	sync_cout << "time_ms=" << time_ms << sync_endl;
	sync_cout << "inc_ms=" << inc_ms << sync_endl;
	sync_cout << "num_matches=" << num_matches << sync_endl;
	sync_cout << "max_moves_to_draw=" << max_moves_to_draw << sync_endl;
	sync_cout << "max_search_per_position=" << max_search_per_position << sync_endl;
	sync_cout << "resign_value=" << resign_value << sync_endl;

	InternalBook internal_book;
	ReadInternalBook("book\\" + output_book_file, internal_book);

	time_t last_save_time_sec = std::time(nullptr);

	std::random_device random_device;
	std::mt19937_64 mt(random_device());

	for (int match_index = 0; match_index < num_matches; ++match_index) {
		sync_cout << "Match:" << match_index << "/" << num_matches << sync_endl;
		std::vector<InternalMove> internal_moves;

		Position pos;
		StateListPtr states(new StateList(1));
		{
			std::istringstream iss("startpos");
			position_cmd(pos, iss, states);
		}

		is_ready();

		int black_time_ms = time_ms;
		int white_time_ms = time_ms;

		while (pos.game_ply() < max_moves_to_draw &&
			!pos.is_mated() &&
			pos.DeclarationWin() == MOVE_NONE &&
			(internal_moves.empty() || internal_moves.back().value == Value::VALUE_NONE || std::abs(internal_moves.back().value) < resign_value) &&
			pos.is_repetition() == RepetitionState::REPETITION_NONE) {
			InternalMove internal_move = {};
			internal_move.best = Move::MOVE_NONE;
			internal_move.next = Move::MOVE_NONE;
			internal_move.value = Value::VALUE_NONE;
			auto sfen = pos.sfen();

			//sync_cout << "sfen=" << pos.sfen() << sync_endl;

			// この局面で探索した回数を調べる。
			// 探索した場合は、評価値を記録するため、その個数を調べる。
			int num_searches = 0;
			if (auto it = internal_book.find(sfen); it != internal_book.end()) {
				for (const auto& [best16, internal_book_move] : it->second) {
					num_searches += internal_book_move.num_values;
				}
			}

			if (num_searches < max_search_per_position){
				// この局面で探索した回数が一定値以下の場合、探索を行う。

				// goコマンドを生成して実行する
				std::string go_command = "go";
				go_command += " btime " + std::to_string(black_time_ms);
				go_command += " wtime " + std::to_string(white_time_ms);
				go_command += " binc " + std::to_string(inc_ms);
				go_command += " winc " + std::to_string(inc_ms);

				//sync_cout << go_command << sync_endl;

				// goコマンドを実行する
				{
					std::istringstream iss(go_command);
					go_cmd(pos, iss, states);
				}

				// goコマンドを待機する
				Threads.main()->wait_for_search_finished();

				// 残り時間を更新する
				TimePoint elapsed = Time.elapsed() + 1;
				if (pos.game_ply() % 2 == 1) {
					// 先手
					black_time_ms += inc_ms;
					black_time_ms -= static_cast<int>(elapsed);
				}
				else {
					white_time_ms += inc_ms;
					white_time_ms -= static_cast<int>(elapsed);
				}

				// 選ばれた指し手のスコアをこの局面のスコアとして記録する
				// 終局している場合はmated_in(0)が代入されている。
				const auto& root_moves = Threads.main()->rootMoves;

				// 指し手と評価値を保存する
				internal_move.best = root_moves[0].pv[0];
				if (root_moves[0].pv.size() > 1) {
					internal_move.next = root_moves[0].pv[1];
				}
				internal_move.value = root_moves[0].score;
				internal_move.depth = Threads.get_best_thread()->completedDepth;
				internal_move.book = 0;
			}
			else {
				// 定跡の指し手を指す
				auto it = internal_book.find(sfen);
				ASSERT_LV3(it != internal_book.end());
				//sync_cout << "Selecting a move with the book." << sync_endl;

				// 全シミュレーション回数を求める
				int N = 0;
				for (const auto& [best16, internal_book_move] : it->second) {
					N += internal_book_move.num_win;
					N += internal_book_move.num_lose;
				}

				// 最大の UCB1 を求める。
				double best_ucb1 = 0.0;
				InternalBookMove best_internal_book_move;
				for (const auto& [best16, internal_book_move] : it->second) {
					double w = internal_book_move.num_win;
					double n = internal_book_move.num_win + internal_book_move.num_lose;
					double ucb1 = w / n + ucb1_constant * std::sqrt(std::log(N) / n);
					//sync_cout << "w=" << w << " n=" << n << " ucb1=" << ucb1 << sync_endl;
					if (best_ucb1 < ucb1) {
						best_ucb1 = ucb1;
						best_internal_book_move = internal_book_move;
					}
				}

				//その手を指す
				internal_move.best = best_internal_book_move.move.to_u16();
				internal_move.next = best_internal_book_move.ponder.to_u16();
				internal_move.value = Value::VALUE_NONE;
				internal_move.book = 1;
				internal_move.depth = 0;
			}

			internal_moves.push_back(internal_move);

			std::string position_command = "startpos";
			if (!internal_moves.empty()) {
				position_command += " moves";
			}
			for (auto move : internal_moves) {
				position_command += " ";
				position_command += to_usi_string(move.best);
			}
			{
				std::istringstream iss(position_command);
				position_cmd(pos, iss, states);
			}
		}

		// 終局処理
		// 現局面のプレイヤーが勝ったかどうか。
		bool current_player_is_win;
		RepetitionState repetition_state = pos.is_repetition(0);
		if (pos.is_mated()) {
			// 負け
			// 詰まされた
			current_player_is_win = false;
		}
		else if (pos.DeclarationWin() != MOVE_NONE) {
			// 勝ち
			// 入玉勝利
			current_player_is_win = true;
		}
		else if (!internal_moves.empty() && internal_moves.back().value >= resign_value) {
			// 勝ち
			current_player_is_win = false;
		}
		else if (!internal_moves.empty() && internal_moves.back().value <= -resign_value) {
			// 負け
			current_player_is_win = true;
		}
		else if (repetition_state == RepetitionState::REPETITION_WIN)
		{
			// 連続王手の千日手による勝ち
			current_player_is_win = true;
		}
		else if (repetition_state == RepetitionState::REPETITION_LOSE)
		{
			// 連続王手の千日手による負け
			current_player_is_win = false;
		}
		else if (repetition_state == RepetitionState::REPETITION_SUPERIOR)
		{
			// 優等局面
			current_player_is_win = true;
		}
		else if (repetition_state == RepetitionState::REPETITION_INFERIOR)
		{
			// 劣等局面
			current_player_is_win = false;
		}
		else {
			// 引き分け
			// この対局は定跡データベースに記録しない。
			continue;
		}

		// 先手が勝ったかどうか
		bool win = current_player_is_win;
		if (internal_moves.size() % 2 == 1) {
			win = !win;
		}

		// 定跡データベースに追加していく
		states.reset(new StateList(1));
		pos.set_hirate(&states->back(), Threads.main());
		for (int play = 0; play < static_cast<int>(internal_moves.size()); ++play) {
			auto sfen = pos.sfen();
			auto move = internal_moves[play].best;
			auto value = internal_moves[play].value;
			auto& internal_book_move = internal_book[sfen][move];
			internal_book_move.move = move;
			if (play + 1 < static_cast<int>(internal_moves.size())) {
				internal_book_move.ponder = internal_moves[play + 1].best;
			}

			if (win) {
				++internal_book_move.num_win;
			}
			else {
				++internal_book_move.num_lose;
			}

			if (value != Value::VALUE_NONE) {
				++internal_book_move.num_values;
				internal_book_move.sum_values += value;
			}

			states->emplace_back();
			pos.do_move(pos.to_move(move), states->back());
			win = !win;
		}

		if (last_save_time_sec + kSavePerAtMostSec < std::time(nullptr)) {
			WriteInternalBook("book\\" + output_book_file, internal_book);
			last_save_time_sec = std::time(nullptr);
		}

		WriteRecordToSQLite(record_file, win ? 0 : 1, internal_moves);
	}

	return true;
}

bool Tanuki::ConvertInternalBookToYaneuraOuBook() {
	std::string csa_folder = Options[kBookCsaFolder];
	std::string input_book_file = Options[kBookInputFile];
	std::string output_book_file = Options[kBookOutputFile];
	int minimum_winning_percentage = static_cast<int>(Options[kBookMinimumWinningPercentage]);
	int minimum_count = static_cast<int>(Options[kBookMinimumCount]);

	MemoryBook output_book;
	output_book_file = "book/" + output_book_file;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book(output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body().size() << sync_endl;

	InternalBook internal_book;
	ReadInternalBook("book\\" + input_book_file, internal_book);
	RemoveBadMove(internal_book);
	RemoveBadMove2(csa_folder, internal_book);

	for (auto& [sfen, best16_to_book_move] : internal_book) {
		for (auto& [best16, book_move] : best16_to_book_move) {
			auto move = book_move.move;
			auto ponder = book_move.ponder;
			int value = book_move.num_values ? static_cast<int>(book_move.sum_values / book_move.num_values) : 0;
			int count = book_move.num_win + book_move.num_lose;
			auto color = (sfen.find(" b ") != std::string::npos ? BLACK : WHITE);

			// 勝率が一定値以下の指し手を削除する。
			// book_move.num_win / count < minimum_winning_percentage / 100
			if (book_move.num_win * 100 < minimum_winning_percentage * count) {
				continue;
			}

			// 出現回数が一定値以下の指し手を削除する。
			if (count < minimum_count) {
				continue;
			}

			auto& position = Threads[0]->rootPos;
			StateInfo state_info;
			position.set(sfen, &state_info, Threads[0]);
			auto move32 = position.to_move(move);
			if (!position.pseudo_legal(move32) || !position.legal(move32)) {
				sync_cout << "Illegal move. sfen=" << position.sfen() << " move=" << move32 << sync_endl;
				continue;
			}

			output_book.insert(sfen, Book::BookMove(move, ponder, value, 0, count));
		}
	}

	WriteBook(output_book, output_book_file);

	return true;
}

#endif
