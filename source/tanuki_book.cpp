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

namespace {
	constexpr const char* kBookInputFile = "BookInputFile";
	constexpr const char* kBookOutputFile = "BookOutputFile";
	constexpr const char* kBookCsaFolder = "BookCsaFolder";
	constexpr const char* kBookTanukiColiseumLogFolder = "BookTanukiColiseumLogFolder";
	constexpr const char* kBookMinimumWinningPercentage = "BookMinimumWinningPercentage";
	constexpr const char* kBookBlackMinimumValue = "BookBlackMinimumValue";
	constexpr const char* kBookWhiteMinimumValue = "BookWhiteMinimumValue";
	constexpr const char* kBookMinimumCount = "BookMinimumCount";
	constexpr const char* kBookMinimumRating = "BookMinimumRating";

	using BadMove = std::pair<std::string, std::string>;
	static const std::vector<BadMove> BadMoves = {
		{"lnsgk1snl/1r4gb1/p1pppp1pp/1p4p2/7P1/2P6/PP1PPPP1P/1BG4R1/LNS1KGSNL w - 8", "8d8e"},
		{"lnsgkgsnl/1r5b1/p1pppp1pp/1p4p2/7P1/2P6/PP1PPPP1P/1B5R1/LNSGKGSNL w - 6", "8d8e"},
		{"lnsgkgsnl/1r5b1/ppppppppp/9/9/7P1/PPPPPPP1P/1B5R1/LNSGKGSNL w - 2", "4a3b"},
		{"ln5nl/1r2gkg2/3ppp1p1/p2s1sp1p/1pp4P1/2PPSPP2/PPS1P1N1P/2GK1G3/LN5RL b Bb 35", "7f7e"},
		{"ln5nl/4gkg2/4ppsp1/p2p2p1p/5P1P1/1rPPS1P2/P3P1N1P/2GK1G3/LN5RL b BSPbs2p 49", "P*8g"},
		{"ln5nl/1r2gkg2/3ppp1p1/p4sp1p/1ps4P1/3PSPP2/PPS1P1N1P/2GK1G3/LN5RL b BPbp 37", "4f4e"},
		// 第3回世界将棋AI電竜戦本戦【予選リーグ】 9回戦 ●Joyful Believer ― 〇dlshogi with HEROZ 30b
		{"ln1gk2nl/1r4g2/3ppps1p/6pp1/pps4PP/2pP1SP2/PPS1PP3/2G4R1/LN2KG1NL b Bbp 35", "7g6h"},
		//20231222 ikari追加
		{"ln1g3nl/1rs2kgs1/2pppp3/p6Rp/1p3+b3/P1P5P/1PSPPPP2/2G1K1S2/LN3G1NL w B2Pp 1", "P*2c"}, //https://denryu-sen.jp/denryusen/dr4_production/dist/#/dr4prd+buoy_blackbid300_dr4a-8-bottom_4_suishoo_tanuki-600-2F+suishoo+tanuki+20231203181535/34
	};

	using GoodMove = std::pair<std::string, std::string>;
	static const std::vector<GoodMove> GoodMoves = {
		{"lr5nl/3gk1g2/2n1ppsp1/p1pps3p/1P4SP1/P1PP4P/2SGPP3/2G4R1/LNK4NL w B3Pb 43", "8a8e"},
		// 後手角換わりを拒否する指し手
		{"lnsgkgsnl/1r5b1/ppppppppp/9/9/2P6/PP1PPPPPP/1B5R1/LNSGKGSNL w - 2", "3c3d"},
		{"lnsgkgsnl/1r5b1/p1ppppppp/1p7/9/2P4P1/PP1PPPP1P/1B5R1/LNSGKGSNL w - 4", "3c3d"},
		{"lnsgkgsnl/1r5b1/p1ppppppp/9/1p5P1/2P6/PP1PPPP1P/1B5R1/LNSGKGSNL w - 6", "3c3d"},
		{"lnsgkgsnl/1r5b1/ppppppppp/9/9/7P1/PPPPPPP1P/1B5R1/LNSGKGSNL w - 2", "8c8d"},
		{"lnsgk1snl/1r4gb1/p1ppppppp/9/1p5P1/2P6/PP1PPPP1P/1BG4R1/LNS1KGSNL w - 8", "7a6b"},
		{"lnsgkgsnl/1r5b1/p1ppppppp/1p7/7P1/9/PPPPPPP1P/1B5R1/LNSGKGSNL w - 4", "8d8e"},
		// ikariさんに指摘された局面
		{"lnsgkgsnl/1r5b1/p1pppp1pp/6p2/1p5P1/2P6/PP1PPPP1P/1BG4R1/LNS1KGSNL w - 8", "4a3b"},
		// 電竜戦 - 棋譜中継 https://denryu-sen.jp/denryusen/dr4_production/dist/#/dr4prd+buoy_blackbid300_dr4y-7-top_4_wanderer_tanuki-600-2F+wanderer+tanuki+20231202170029/26
		// 7四歩
		{"ln1g3nl/1r1s1kgs1/p1ppppb2/6R1p/1p7/2P5P/PPBPPPP2/1SG1K4/LN3GSNL w 2Pp 22", "7c7d"},
		// 電竜戦 - 棋譜中継 https://denryu-sen.jp/denryusen/dr4_production/dist/#/dr4prd+buoy_blackbid300_dr4y-2-bottom_4_nibanshibori_tanuki-600-2F+nibanshibori+tanuki+20231202113050/10
		// 1四歩
		{"lnsgkgsnl/1r5b1/p1ppppppp/9/1p5P1/2P6/PP1PPPP1P/1B5R1/LNSGKGSNL w - 6", "1c1d"},

		//20231222 ikari追加
		{"lnsgk1snl/1r4gb1/p1pppp2p/6pR1/1p7/2P6/PP1PPPP1P/1BG6/LNS1KGSNL w Pp 1", "8e8f"}, //https://denryu-sen.jp/denryusen/dr4_production/dist/#/dr4prd+buoy_blackbid300_dr4y-2-bottom_4_nibanshibori_tanuki-600-2F+nibanshibori+tanuki+20231202113050/16 
		{"lnsgk1snl/1r4gb1/p1pppp3/6pRp/1p7/2P6/PPBPPPP1P/9/LNSGKGSNL b Pp 1", "7i8h"}, //https://denryu-sen.jp/denryusen/dr4_production/dist/#/dr4prd+buoy_blackbid300_dr4y-5-bottom_4_tanuki_dlshogi-600-2F+tanuki+dlshogi+20231202153201/17
		{"ln1g3nl/1r1s1kgs1/p1pppp3/6R2/1p6P/2P6/PPSPPPP2/2G1K4/LN3GSNL w B3Pbp 1", "P*1h"}, //https://denryu-sen.jp/denryusen/dr4_production/dist/#/dr4prd+buoy_blackbid300_dr4y-7-top_4_wanderer_tanuki-600-2F+wanderer+tanuki+20231202170029/30
		{"lnsgkgsnl/1r5b1/pppppp1pp/6p2/9/2P4P1/PP1PPPP1P/1B5R1/LNSGKGSNL w - 1", "8d8e"}, //振り飛車拒否
		{"ln1g3nl/1r3kgs1/p2p1p3/3s1b2p/1pP1p4/2p3P1P/PP1PPPS2/1SGBK2R1/LN3G1NL w 3P 1", "6d7e"}, //https://denryu-sen.jp/denryusen/dr4_production/dist/#/dr4prd+buoy_blackbid300_dr4a-9-top_4_wanderer_tanuki-600-2F+wanderer+tanuki+20231203185014/46
		// 竜王戦3組ランキング戦、阿久津八段対大橋七段戦
		{"lr5nl/3g1kgs1/2n1p2p1/3Ps1P1p/P4P1P1/2S1S3P/1G2P4/1K3G3/LN5RL b B6Pbn2p 0", "P*8f"},
		// 【#電竜戦 最終日午後 水匠視点】2日日午後！目指せ連覇！将棋AI界最強の座！！【将棋AI水匠／たややん】 - YouTube https://www.youtube.com/watch?v=wiqNmUq00kk
		{"l4rknl/3g2g2/2n1p1sp1/p1ppspp1p/1p3P1P1/P1PPS1P1P/1PS1P1N2/1KG2G3/LN5RL b Bb 1", "2e2d"},
	};

	void WriteBook(Book::MemoryBook& book, const std::filesystem::path& output_book_file_path) {
		std::filesystem::path backup_file_path = output_book_file_path;
		backup_file_path += ".bak";

		if (std::filesystem::exists(backup_file_path)) {
			sync_cout << "Removing the backup file. backup_file_path=" << backup_file_path << sync_endl;
			std::filesystem::remove(backup_file_path);
		}

		if (std::filesystem::exists(output_book_file_path)) {
			sync_cout << "Renaming the output file. output_book_file_path=" << output_book_file_path << " backup_file_path=" << backup_file_path << sync_endl;
			std::filesystem::rename(output_book_file_path, backup_file_path);
		}

		book.write_book(output_book_file_path.string());
		sync_cout << "|output_book_file_path|=" << book.get_body().size() << sync_endl;
	}

	struct Player {
		std::string name;
		int rate;
	};

	bool ReadStrongPlayers(std::vector<Player>& strong_players) {
		sync_cout << "ReadStrongPlayers()" << sync_endl;
		std::map<std::string, int> name_to_rate;
		int processed = 0;
		for (const auto& entry : std::filesystem::directory_iterator("../rating")) {
			auto file_path = entry.path().string();
			if (file_path.find("players-floodgate-2020") == std::string::npos &&
				file_path.find("players-floodgate-2021") == std::string::npos &&
				file_path.find("players-floodgate-2022") == std::string::npos &&
				file_path.find("players-floodgate-2023") == std::string::npos &&
				file_path.find("players-floodgate-2024") == std::string::npos) {
				continue;
			}

			if (++processed % 100 == 0) {
				sync_cout << processed << sync_endl;
			}

			std::ifstream ifs(file_path);
			if (!ifs) {
				continue;
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

					name_to_rate[name] = std::max(name_to_rate[name], rate);
				}
			}
		}

		for (auto [name, rate] : name_to_rate) {
			strong_players.push_back({ name, rate });
		}

		if (strong_players.empty()) {
			return false;
		}

		return true;
	}
}

bool Tanuki::InitializeBook(USI::OptionsMap& o) {
	o[kBookInputFile] << Option("user_book1.db");
	o[kBookOutputFile] << Option("user_book2.db");
	o[kBookCsaFolder] << Option("");
	o[kBookTanukiColiseumLogFolder] << Option("");
	o[kBookMinimumWinningPercentage] << Option(0, 0, 100);
	o[kBookBlackMinimumValue] << Option(-VALUE_MATE, -VALUE_MATE, VALUE_MATE);
	o[kBookWhiteMinimumValue] << Option(-VALUE_MATE, -VALUE_MATE, VALUE_MATE);
	o[kBookMinimumCount] << Option(0, 0, INT_MAX);
	o[kBookMinimumRating] << Option(3300, 0, INT_MAX);

	return true;
}

// 複数の定跡をマージする
// BookInputFileには「;」区切りで定跡データベースのフルパスを指定する
// BookOutputFileにはbook以下のファイル名を指定する
bool Tanuki::MergeBook() {
	sync_cout << "MergeBook()" << sync_endl;

	std::string input_file_list = Options[kBookInputFile];
	std::string output_file = Options[kBookOutputFile];

	sync_cout << "info string input_file_list=" << input_file_list << sync_endl;
	sync_cout << "info string output_file=" << output_file << sync_endl;

	MemoryBook output_book;
	sync_cout << "Reading output book file: " << output_file << sync_endl;
	output_book.read_book("book/" + output_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body().size() << sync_endl;

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
		MemoryBook input_book;
		sync_cout << "Reading input book file: " << input_file << sync_endl;
		input_book.read_book("book/" + input_file);
		sync_cout << "done..." << sync_endl;
		sync_cout << "|input_book|=" << input_book.get_body().size() << sync_endl;

		for (const auto& book_type : input_book.get_body()) {
			const auto& sfen = book_type.first;
			const auto& pos_move_list = book_type.second;

			if (input_file_index == 1) {
				Position& position = Threads[0]->rootPos;
				StateInfo state_info;
				position.set(sfen, &state_info, Threads[0]);
				if (position.side_to_move() == WHITE) {
					continue;
				}
			}

			output_book.get_body()[sfen] = pos_move_list;
		}
	}

	WriteBook(output_book, std::filesystem::path("book") / output_file);

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

	u16 to_move16(const std::string& str) {
		if (str == "none") {
			return static_cast<u16>(Move::MOVE_NONE);
		}
		return USI::to_move16(str).to_u16();
	}

	void ReadInternalBook(const std::filesystem::path& file_path, InternalBook& internal_book) {
		sync_cout << "ReadInternalBook(): file_path=" << file_path << sync_endl;
		int counter = 0;
		std::ifstream ifs(file_path);
		if (!ifs) {
			sync_cout << "!!! Failed to read an internal book file" << sync_endl;
			std::exit(-1);
		}
		std::string sfen;
		while (std::getline(ifs, sfen)) {
			if (++counter % 100000 == 0) {
				sync_cout << counter << sync_endl;
			}

			int num_book_moves;
			ifs >> num_book_moves;

			// 末尾の改行を読む
			std::string _;
			std::getline(ifs, _);

			for (int book_move_index = 0; book_move_index < num_book_moves; ++book_move_index) {
				std::string line;
				std::getline(ifs, line);
				std::istringstream iss(line);

				std::string move_string;
				std::string ponder_string;
				int num_win;
				int num_lose;
				s64 sum_values;
				int num_values;
				if (line.find("  ") == std::string::npos) {
					iss >> move_string >> ponder_string >> num_win >> num_lose >> sum_values >> num_values;
				}
				else {
					// 何らかの原因でponderが空文字の場合がある。この場合noneが含まれていると仮定して読み込む。
					ponder_string = "none";
					iss >> move_string >> num_win >> num_lose >> sum_values >> num_values;
				}
				u16 move16 = USI::to_move16(move_string).to_u16();
				u16 ponder16 = ponder_string == "none" ? static_cast<u16>(Move::MOVE_NONE) : USI::to_move16(ponder_string).to_u16();
				internal_book[sfen][move16] = { move16, ponder16, num_win, num_lose, sum_values, num_values };
			}

		}
		sync_cout << "counter=" << counter << " done." << sync_endl;
	}

	void WriteInternalBook(const std::filesystem::path& file_path, const InternalBook& internal_book) {
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

bool Tanuki::CreateTayayanBook2() {
	sync_cout << "CreateTayayanBook2()" << sync_endl;

	std::string csa_folder = Options[kBookCsaFolder];
	std::string output_book_file = Options[kBookOutputFile];
	int minimum_winning_percentage = static_cast<int>(Options[kBookMinimumWinningPercentage]);
	int black_minimum_value = static_cast<int>(Options[kBookBlackMinimumValue]);
	int white_minimum_value = static_cast<int>(Options[kBookWhiteMinimumValue]);
	int minimum_count = static_cast<int>(Options[kBookMinimumCount]);
	int minimum_rating = static_cast<int>(Options[kBookMinimumRating]);

	MemoryBook output_book;
	sync_cout << "Reading output book file: " << output_book_file << sync_endl;
	output_book.read_book("book/" + output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << output_book.get_body().size() << sync_endl;

	std::vector<Player> strong_players;
	if (!ReadStrongPlayers(strong_players)) {
		sync_cout << "Failed to read the player list." << sync_endl;
		return false;
	}

	InternalBook internal_book;
	ParseFloodgateCsaFiles(csa_folder, strong_players, minimum_rating, internal_book);

	sync_cout << "Reading csa files..." << sync_endl;
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

	WriteBook(output_book, "book/" + output_book_file);

	return true;
}

namespace {
	void RemoveMoveAndMaybePosition(Book::BookType& book, const std::string& sfen, const std::string& move_string) {
		auto position_and_book_moves = book.find(sfen);
		if (position_and_book_moves == book.end()) {
			sync_cout << "Falied to remove a bad move. Position was not found. sfen=" << sfen << " move=" << move_string << sync_endl;
			return;
		}

		// 指し手を削除する
		u16 move16 = USI::to_move16(move_string).to_u16();
		auto& book_moves = position_and_book_moves->second;
		auto find_book_move = [move16](const Book::BookMove& move) {
			return move.move == move16;
		};
		auto book_move = std::find_if(book_moves->begin(), book_moves->end(), find_book_move);
		bool found = false;
		while (book_move != book_moves->end()) {
			found = true;
			sync_cout << "Removed a bad move. sfen=" << sfen << " move=" << move_string << sync_endl;
			book_moves->erase(book_move, book_move + 1);
			book_move = std::find_if(book_moves->begin(), book_moves->end(), find_book_move);
		}

		if (!found) {
			sync_cout << "Falied to remove a bad move. Move was not found. sfen=" << sfen << " move=" << move_string << sync_endl;
		}

		// 指し手が空になった局面を削除する。
		if (book_moves->size() == 0) {
			book.erase(position_and_book_moves);
			sync_cout << "Removed a position. sfen=" << sfen << sync_endl;
		}
	}
}

/// <summary>
/// 悪い指し手を削除する。
/// </summary>
void Tanuki::RemoveBadMove() {
	sync_cout << "RemoveBadMove()" << sync_endl;

	std::string input_book_file = Options[kBookInputFile];
	std::string output_book_file = Options[kBookOutputFile];

	MemoryBook book;
	sync_cout << "Reading input book file: " << input_book_file << sync_endl;
	book.read_book("book/" + input_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|input_book|=" << book.get_body().size() << sync_endl;

	// typedef std::shared_ptr<BookMoves> BookMovesPtr;
	// typedef std::unordered_map<std::string /* sfen */, BookMovesPtr > BookType;
	for (const auto& [sfen, move_string] : BadMoves) {
		RemoveMoveAndMaybePosition(book.get_body(), sfen, move_string);
	}

	sync_cout << "Writing output book file: " << output_book_file << sync_endl;
	WriteBook(book, "book/" + output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << book.get_body().size() << sync_endl;
}

void Tanuki::RemoveBadMove2() {
	sync_cout << "RemoveBadMove2()" << sync_endl;

	std::string csa_folder = Options[kBookCsaFolder];
	std::string input_book_file = Options[kBookInputFile];
	std::string output_book_file = Options[kBookOutputFile];

	MemoryBook book;
	sync_cout << "Reading input book file: " << input_book_file << sync_endl;
	book.read_book("book/" + input_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|input_book|=" << book.get_body().size() << sync_endl;

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

		RemoveMoveAndMaybePosition(book.get_body(), sfen, move_string);
	}

	sync_cout << "Writing output book file: " << output_book_file << sync_endl;
	WriteBook(book, "book/" + output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << book.get_body().size() << sync_endl;
}


void Tanuki::AddGoodMove() {
	sync_cout << "AddGoodMove()" << sync_endl;

	std::string input_book_file = Options[kBookInputFile];
	std::string output_book_file = Options[kBookOutputFile];

	MemoryBook book;
	sync_cout << "Reading input book file: " << input_book_file << sync_endl;
	book.read_book("book/" + input_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|input_book|=" << book.get_body().size() << sync_endl;

	for (auto& [sfen, move_string] : GoodMoves) {
		if (book.get_body().erase(sfen) > 0) {
			sync_cout << "Removed a position. sfen=" << sfen << sync_endl;
		}
		Move16 move16 = USI::to_move16(move_string);
		book.insert(sfen, Book::BookMove(move16, Move::MOVE_NONE, 0, 0, 1));

		sync_cout << "Added a good move. sfen=" << sfen << " move=" << move_string << sync_endl;
	}

	sync_cout << "Writing output book file: " << output_book_file << sync_endl;
	WriteBook(book, "book/" + output_book_file);
	sync_cout << "done..." << sync_endl;
	sync_cout << "|output_book|=" << book.get_body().size() << sync_endl;
}

#endif
