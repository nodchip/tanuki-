#include "denryu2_investigation.h"

#include <filesystem>
#include <random>
#include <cassert>

#include "tanuki_progress.h"
#include "position.h"
#include "thread.h"
#include "csa.h"

namespace {
	std::vector<std::string> GamesForPreliminaryExperiment = {
		"7g7f 3c3d 2g2f 8b4b 2f2e 5a6b 3i4h 6b7b 5i6h 2b8h+ 7i8h 3a3b 6h7h 3b3c 8h7g 7b8b 5g5f 7a7b 4i5h 9c9d 9g9f 7c7d 1g1f 6c6d 3g3f 4a5b 4h5g 5b6c 6g6f 8a7c 5h6g 4c4d 5f5e 1c1d 6i6h 8c8d 5g5f 4d4e 6h5g 7b8c 2i3g 6a7b 3g4e 3c4d 4g4f 3d3e 2e2d 3e3f 2d2c+ 3f3g+ 2h2d P*4g 2d3d B*3f 3d3a+ 4b6b P*3h 6d6e 6f6e 3f2g+ 3h3g 4g4h+ B*5a 6b5b 5a2d+ 2g4i 2d3d 6c6b 3d5b 6b5b R*8a 8b9c 8a9a+ B*9b 3a8a 7c8e 9a9b 8c9b 8a7b 8e7g+ 8i7g S*9a N*8e 8d8e 7b7c 9b8c G*8d 9c9b 7c8c",
		"7g7f 8c8d 2g2f 4a3b 7i7h 3c3d 7h7g 3a4b 3i4h 7c7d 2f2e 4b3c 5g5f 7a6b 5i6h 8a7c 6h7h 6c6d 3g3f 6b6c 8h7i 6a5b 4i5h 5a4a 6g6f 1c1d 5h6g 6c5d 4h3g 8b6b 3g4f 3c4d 2i3g 7c8e 7g8f 6d6e 7i8h 4d4e 6f6e 2b8h+ 7h8h 4e4f 4g4f B*4g B*5h S*3i 5h4g 3i2h 3g4e 2h3g+ 4g5h 3g4h B*7c 5d4e 7c6b+ 5b6b R*8a N*5a 4f4e B*3c 4e4d R*2h S*5i 4h5i 6i5i S*4g 6g6h 4g5h+ 5i5h 3c4d S*5e P*6g P*4b 3b4b 5h6g P*6f S*7h 6f6g+ 6h6g 4d5e 5f5e S*6i S*6h 6i7h+ 8h7h S*5h 6g5f 5h6i 7h6g B*7h 6g5g 2h5h+ 5g4f G*4g 4f4e 7h5f+",
		"6i7h 8c8d 7g7f 8d8e 8h7g 3c3d 7i6h 2b7g+ 6h7g 3a2b 9g9f 7a7b 3i3h 2b3c 3g3f 4a3b 2g2f 7c7d 2f2e 9c9d 2i3g 7b7c 4g4f 7c6d 4i4h 4c4d 2h2i B*5d 5i6h 7d7e 7f7e 3d3e 3f3e 6d7e 3e3d 3c3d B*5e 7e6d 5e4d 8e8f 8g8f P*3c P*7f 9d9e 9f9e P*7e 7g8h 8b8f P*8g 8f8c 2e2d 2c2d 2i2d 3d2c 2d2f 7e7f P*2b P*2e 2f2e 3b2b 9e9d P*9h 9i9h 2b3b 2e9e P*2g P*2b 3b2b 3g4e 5a4b 3h4g 2c3d 9d9c+ 8a9c P*9d 9c8e 9d9c+ 3d4e 4f4e 7f7g+ 8i7g P*4f 4g4f P*4g 4h4g 9a9c 9e9c+ N*7f 6h5h 8c9c 9h9c+ R*2h S*4h 7f8h+ 7h8h 8e7g 4d7g 5d6e 5g5f S*3i N*3e N*3f L*4d P*4c 3e4c+ 6e4c 4d4c+ 4b4c 4e4d 4c5b 4d4c+ 5b4c P*4i 3f4h+ 4i4h 4c5b N*4d 5b4b P*2c L*5g 4f5g N*3e 2c2b+ 3e4g+ 5h6i 4g5h 6i7h 5h6h 5g6h S*6i 7h6i G*5h 6i7i 5h6i 7i8i 6i7i 6h7i P*7h S*3a 4b4c B*3b 4c3d R*3f 3d2e G*3e 2e1e 1g1f",
		"7g7f 8c8d 2g2f 8d8e 8h7g 3c3d 7i7h 4a3b 3g3f 7a6b 2f2e 6c6d 3i3h 6b6c 2i3g 2b7g+ 7h7g 3a2b 5i6h 5a4b 2e2d 2c2d 2h2d 8e8f 8g8f B*3c 2d3d P*8h 3d3c+ 2b3c 7g8h 8b8f 6i7h 8f8b 4g4f 7c7d B*6f 8a7c 3g2e P*3g 3h3g 6a7b 2e3c+ 2a3c 3f3e R*2i B*3h 2i1i+ S*2h 1i1h 3h2g 1h2g 2h2g 6d6e 6f7g 6e6f 7g6f L*6d 3e3d 6d6f 3d3c+ 3b3c 6g6f N*5e P*8g P*6g 6h5i B*6h 7h6h 6g6h+ 5i6h G*6g 6h5i B*6h 5i4h 6h5g+ 4h3h 5e4g+ 3h2h 4g3g 2h3g S*3e N*3h 8b8a L*3f 3e3f 2g3f 4b5b P*3d 3c3b P*5d 8a2a 3d3c+ P*2f 3c3b 2f2g+ 3f2g L*3e 3g2h 6c5d 3b2a 3e3h+ 4i3h 5b6c R*3b 6c6d B*8f N*7e 3b7b+ 6g6f L*5i P*2f S*5e 6d5e 5i5g 6f5g G*6f 5e6f 8f7g 6f5f G*6f 5f4f R*3f 4f4e P*4f 4e4d B*3c",
		"2g2f 8c8d 7g7f 8d8e 2f2e 4a3b 8h7g 3c3d 7i8h 2b7g+ 8h7g 3a2b 6i7h 2b3c 3i3h 7a7b 3g3f 4c4d 5i6h 7c7d 3h3g 8a7c 3g4f 8b8a 6g6f 9c9d 4i5h 5a6b B*5f 8a8d 9g9f 5c5d 3f3e 3d3e 4f3e P*3h 2e2d 2c2d 3e2d B*6d P*3g 8e8f 8g8f P*2b 2d3c+ 2a3c P*2c 5d5e 5f3d 2b2c S*4c S*3a 4c3b 3a3b G*5d 3h3i+ 3d2c+ 3i3h 5d6d 6c6d 2h2d 3b2c 2d2c+ G*4h 5h4h 3h4h 2c3c S*5b B*3d 6b7a 3c3a 7a8b 3a1a 5e5f 3d5b+ 5f5g+ 6h5g 4h4g 5g4g 6a5b S*7a 8b9c S*8b 8d8b 7a8b 9c8b R*8a 7b8a G*8c 8b8c 1a8a P*8b L*8d 8c8d 8a8b R*8c 8f8e 8d8e 8b8c B*8d S*8f",
		"7g7f 3c3d 2g2f 8c8d 2f2e 8d8e 6i7h 4a3b 2e2d 2c2d 2h2d 8e8f 8g8f 8b8f 2d3d 2b3c 5i5h 5a5b 3g3f 8f7f 8i7g 3c5e 3d8d P*8b P*2h 7f3f 7g6e 5e8h+ 7i8h 3a4b B*5e B*3c 5e3c+ 2a3c 3i3h B*5e B*3g 5e3g+ 2i3g 3f3e 8d8e B*6d P*3d 3e3d B*5f 3d2d P*3d 2d2h+ 3d3c+ 3b3c 6e5c+ 6d5c N*4e 5c6d 4e3c+ 4b3c 8e2e 6d3g+ 2e2h 3g2h R*2a 2h5e 5f2c+ 5b6b G*5c 6b5c 2a6a+ P*5f G*6f 5f5g+ 5h5g N*6e 6f6e 5e6e 6a5a 5c6d N*5f 6d7d 6g6f 6e7e 5a5e G*3d 5e7e 7d7e P*7f 7e8d B*5c N*4e 5g6h G*6b 5c7e+ 8d8c P*3e 7c7d 7e7d 8c7d 3e3d N*8f P*5h 3c4d 5f4d 8f7h+ 6h7h 4c4d G*6h N*8f 7h7g 7d8c 2c3b 8a7c S*7e G*8e 3b4b P*5c 4g4f R*8i N*7i B*5f 7i6g R*7i 8h7i 8i7i+ R*7h 8f7h+ 6h7h 8e7f 7g7f 7i7h G*7g G*8e",
		"2g2f 3c3d 7g7f 9c9d 2f2e 9d9e 5i6h 2b8h+ 7i8h 3a2b 6h7h 2b3c 3g3f 8b2b 3i4h 5a6b 4g4f 6b7b 4h4g 4a5b 2i3g 4c4d 4i5h 7b8b 8g8f 7a7b 8h8g 8c8d 6i6h 7b8c 8i7g 6a7b 9g9f 9e9f 9i9f P*9d 2h2i 6c6d 2i9i 7c7d B*6f 8a7c 4g5f 2b4b 4f4e 5c5d 4e4d 3c4d 8f8e 8d8e P*8d 8c9b 9i4i P*4c 2e2d 5d5e 5f5e 6d6e 7g6e 7c6e 5e5d 2c2d 5d6e B*3h 4i4d 4c4d 6f5e N*7c P*4g 5b6c S*6d 6c6d 5e6d 4b6b 9f9d P*9c N*6f S*6c 6d4f 8b7a 6e7d P*6d 4f5e 9c9d 5e4d 8e8f 8g8f R*4a 4d6b+ 7b6b P*9f 3h4g+ 5h4g P*7g 8f7g 4a4g+ R*5a G*6a 5a2a+ B*8e B*8b 7a8a 8d8c+ 9b8c 7d8c 8e9f P*8g 9f8g+ 7h8g L*8d P*8f 8d8f 7g8f 1c1d N*9c 9a9c G*9a",
		"2g2f 8c8d 6i7h 8d8e 2f2e 4a3b 9g9f 9c9d 3i3h 7a7b 5i6h 8e8f 8g8f 8b8f P*8g 8f8b 3g3f 7c7d 2e2d 2c2d 2h2d P*2c 2d7d 7b7c 7d7e 3c3d 7e2e 7c6d 7g7f 8a7c 2i3g 5a4b 4g4f 6a5b 3h4g 3d3e 3f3e 2b8h+ 7i8h B*1d 4g5h 1d2e 3g2e R*2i 9f9e 2i2e+ 9e9d 8b8e 9d9c+ 2e2h P*9b 8e3e 9b9a+ N*5d P*3f 3e3f P*2i 2h1i 9c8c 7c6e 8i7g 5d4f 7g6e 4f5h+ 4i5h 6d6e B*4g 3f3i+ 4g6e S*6i 6h7g 3i3e 6g6f 1i1h N*2h 3e6e 6f6e N*7d S*7e B*5e L*6f 6i7h+ 7g7h 7d6f 7e6f 5e6f N*3d 4b3c S*3e 3b4b 8h7g 6f5e 3d4b+ 3c4b N*3d 4b3b P*4b G*5a B*7c 5e7c 8c7c 1h2i G*4a S*6i 7h8h 5a4a B*5f 4a5a 7c6c 2i2h 6c5b N*8e 8g8f B*7i 8h7i G*7h 5f7h 6i7h+ 7i7h 2h5h 7h8g B*6i S*7h 6i7h+ 8g9h L*9g",
		"7g7f 8c8d 2h6h 1c1d 1g1f 5a4b 6g6f 7a6b 7i7h 6a5b 4i4h 5c5d 6i5h 8d8e 8h7g 3c3d 5i4i 4b3b 4i3h 6b5c 3g3f 7c7d 3i2h 4a4b 2h3g 8a7c 5g5f 9c9d 7h6g 8b8d 6h8h 5c6d 5h5g 7d7e 7f7e 6d7e P*7f 7e6d 3h2h 5d5e 4h3h 8e8f 8g8f 8d7d 5f5e 6d5e 8h5h P*8h P*5f 5e6d 7g8h P*7e 5h7h 7e7f 6g7f 7d7f 7h7f S*8g 7f7d 8g8h 4g4f B*6h 3h4g 8h9i+ 7d8d 9i8i 8d8b+ 6h5i+ 3g4h 5i4i R*6i 4i7f P*7d N*6e 5g6g 7f8g 6i3i 8g7h 6f6e 7c6e P*6h P*5g N*5d 5g5h+ 7d7c+ 6d7c 8b7c 6e7g+ 6g5g 7h6h 5d4b+ 5b4b S*5a 4b4a 7c7b L*4b 5a6b+ P*5a 5g5h 6h5h 3i8i 1d1e G*3h 1e1f P*1d 2b6f 7b6c 7g6g P*6h 5h6h 2g2f 1a1d S*2e G*1e 6c6e 6f4d 2e1d 1e1d L*1h N*2d 1h1f P*6d 6e4e P*1h 1i1h P*1g 1h1g 4d6b 1f1d P*1f 1g1f 2d1f 2h1h L*4d 4e3d P*3c 3d2e 4d4f P*6c 6b4d P*4e 4f4g+ 4h4g 4d7a 2e1f P*1e 1f2g G*1f 2g2h 6g5g G*3g 5g4g 3h4g 6h2d 1d1a+ 3b2b P*1b 3c3d 1h1i 2a3c L*1h S*1d 3f3e 4a3b N*2g 3d3e 8i3i 7a9c 3i4i S*3d N*4f 3c4e 6c6b+ 4c4d P*3c 3b3c P*7e 9c7e 1h1f 1e1f G*2a 2b1c 2a3a L*1g S*1h 4e3g+ 4g3g S*3i 1h1g 1f1g+ 2h1g S*1f 1g1h 1d1e N*2h P*1g 2i1g 3i2h+ 1h2h N*4e S*3h 1f1g+ 2h1g P*1f 1g2h G*1g S*2i 1g2h 2i2h 4e3g+ 3h3g N*3f G*2i G*1g L*1h 3f2h+ 3g2h 1e2f 4f3d 3c3d S*2b 1c2b N*3f 3e3f P*3c 2b1c 2h1g 1f1g+ G*2h 1g1h 2h1h P*1g 3c3b+ 1g1h+ 2i1h P*1g N*2e 3d2e 4i4h 7e4h+ 1h2h 4h3g 1i2i S*1h 2h1h S*3h",
		"7g7f 8c8d 7i6h 3c3d 6h7g 7a6b 5g5f 7c7d 2g2f 6c6d 2f2e 4a3b 2e2d 2c2d 2h2d 8d8e 6i7h 8a7c 8h7i 6b6c 7i6h 5a4a 2d2h P*2c 3i4h 5c5d 4h5g 7c6e 5g4f 6e7g+ 8i7g 6a5b 4f4e 7d7e 7f7e 8e8f 8g8f 8b8f P*8g 8f8a 7e7d 6c7d N*6f 7d7e P*7c 8a8b 4e3d P*7f 3d2c 3b2c 2h2c+ 7f7g+ 6h7g S*5g G*5h N*6e 7g6h P*7g 6h7g 6e7g+ 7h7g 7e6f 6g6f N*6e S*6a 6e7g+ 5h5g S*6h 5i4h 6h5g+ 4h3h B*6g 4i3i 6g5f+ S*3b 3a3b 2c2b 5g4g 3h2g 5f4e 3g3f P*2f 2b2f S*3e 2f3e 4e3e 3f3e P*2f 2g2f R*4f N*3f G*4e B*2e 2a3c S*3d 3c2e 3d4e B*5i P*4h 4g4h G*6i 4h3i 6i5i 4f4e B*7d 4a3a 6a5b+ 8b5b B*1e S*1d G*4b 5b4b 1e4b+ 3a4b 7d5b+ 4b5b 3f4d 4e4d P*5c 5b4a P*4b 4a3a R*6a 3a2b 6a3a+ 2b3c 3e3d 4d3d 3a3b 3c3b P*3c 3b2c P*2d 2c2b 3c3b+ 3d3b S*2c 1d2c 2d2c+ 2b2c P*2d 2c3c P*3d 3c2b S*2c 2b2a 2c3b 2a3b 2d2c+ 3b2c R*3c 2c2d 3c2c+ 2d2c 3d3c+ 2c2d 3c2c 2d2c 2f2e S*2d 2e3f G*3e 3f4g S*4f 4g5h S*5g 5h6i G*7h",
	};
}

void position_cmd(Position& pos, std::istringstream& is, StateListPtr& states);
void go_cmd(const Position& pos, std::istringstream& is, StateListPtr& states, bool ignore_ponder = false);

void Denryu2::Initialize(USI::OptionsMap& o)
{
	o["Target"] << USI::Option("");
}

void Denryu2::ExtractTime()
{
	auto progress = std::make_unique<Tanuki::Progress>();
	if (!progress->Load()) {
		std::exit(1);
	}

	Position pos;

	for (const auto& directory_entry : std::filesystem::directory_iterator("C:\\shogi\\dr2_kifu_csa")) {
		const auto& path = directory_entry.path();
		const auto& filename = path.filename().string();

		Color target_color;
		if (filename.find("_" + static_cast<std::string>(Options["Target"]) + "_") != std::string::npos) {
			target_color = BLACK;
		}
		else if (filename.find("_" + static_cast<std::string>(Options["Target"]) + "-") != std::string::npos) {
			target_color = WHITE;
		}
		else {
			continue;
		}

		int day;
		if (filename.find("+dr2prdy1-") != std::string::npos) {
			day = 1;
		}
		else {
			day = 2;
		}

		std::vector<StateInfo> state_info(512);
		pos.set_hirate(&state_info[0], Threads.main());

		std::ifstream ifs(path);
		std::string line;
		while (std::getline(ifs, line) && line != "+")
			;

		Color color = BLACK;
		while (std::getline(ifs, line) && line.find("%") != 0) {
			auto csa_move_str = line.substr(1);
			std::string time_str;
			std::getline(ifs, time_str);
			auto time = std::atoi(time_str.substr(1).c_str());

			if (target_color == color) {
				auto p = progress->Estimate(pos);
				auto progress_type = static_cast<int>(p * 3.0);
				std::printf("%d,%d,%d\n", day, progress_type, time);
			}

			auto move = CSA::to_move(pos, csa_move_str);
			pos.do_move(move, state_info[pos.game_ply()]);

			color = ~color;
		}
	}

	std::cout << "done" << std::endl;
}

void Denryu2::ExtractGames()
{
	// 棋譜を読み込む
	sync_cout << "Reading records..." << sync_endl;
	std::string book_file = "C:\\shogi\\floodgate\\wdoor.sfen";
	std::ifstream ifs(book_file);
	if (!ifs) {
		sync_cout << "info string Failed to read the progress book file." << sync_endl;
		std::exit(-1);
	}

	std::vector<std::string> valid_games;
	std::string line;
	int index = 0;
	while (std::getline(ifs, line)) {
		std::istringstream iss(line);
		std::string move_str;
		Position pos;
		std::vector<StateInfo> state_info(512);
		pos.set_hirate(&state_info[0], Threads.main());
		while (iss >> move_str) {
			if (move_str == "startpos" || move_str == "moves") {
				continue;
			}

			Move move = USI::to_move(pos, move_str);
			if (!is_ok(move) || !pos.legal(move)) {
				break;
			}

			pos.do_move(move, state_info[pos.game_ply()]);
		}

		if (pos.is_mated() || pos.DeclarationWin() != MOVE_NONE) {
			valid_games.push_back(line);
		}

		if (++index % 10000 == 0) {
			sync_cout << index << sync_endl;
		}
	}

	sync_cout << "num_records: " << valid_games.size() << sync_endl;
	std::random_device rd;
	std::mt19937_64 mt(rd());
	std::shuffle(valid_games.begin(), valid_games.end(), mt);

	for (int game_index = 0; game_index < 10; ++game_index) {
		sync_cout << valid_games[game_index] << sync_endl;
	}
}

void Denryu2::CalculateMoveMatchRatio(Position& pos, std::istringstream& is)
{
	auto progress = std::make_unique<Tanuki::Progress>();
	if (!progress->Load()) {
		sync_cout << "Failed to load progress parameters." << sync_endl;
		std::exit(1);
	}

	int num_trials_per_game = 10;
	int multi_pv = Options["MultiPV"];

	const auto& games = GamesForPreliminaryExperiment;

	for (int game_index = 0; game_index < games.size(); ++game_index) {
		const auto& game = games[game_index];
		for (int trial = 0; trial < num_trials_per_game; ++trial) {
			// 局面を初期化する。
			StateListPtr states(new StateList(1));
			{
				std::istringstream iss("startpos");
				position_cmd(pos, iss, states);
			}
			is_ready();

			std::istringstream iss(game);
			std::string move_str;
			std::string position_command = "startpos";
			while (iss >> move_str) {
				auto move = USI::to_move(pos, move_str);

				int progress_type = static_cast<int>(progress->Estimate(pos) * 3.0);

				// goコマンドを実行する
				{
					std::istringstream iss("go byoyomi 5000");
					go_cmd(pos, iss, states);
				}

				// goコマンドを待機する
				Threads.main()->wait_for_search_finished();

				// 棋譜の指し手が読み筋に含まれているかどうか調べる。
				bool found = false;
				const auto& root_moves = Threads.main()->rootMoves;
				for (int pv_index = 0; !found && pv_index < multi_pv && pv_index < root_moves.size(); ++pv_index) {
					const auto& pv = root_moves[pv_index].pv;
					if (pv.empty()) {
						continue;
					}

					found = (pv[0] == move);
				}

				std::printf("%d,%d,%d,%d,%d\n", game_index, trial, pos.game_ply(), progress_type, found ? 1 : 0);
				std::fflush(nullptr);

				if (pos.game_ply() == 1) {
					position_command += " moves";
				}
				position_command += " ";
				position_command += move_str;

				{
					std::istringstream iss(position_command);
					position_cmd(pos, iss, states);
				}
			}
		}
	}
}

void Denryu2::CalculateMoveMatchRatio2(Position& pos)
{
	auto progress = std::make_unique<Tanuki::Progress>();
	if (!progress->Load()) {
		std::exit(1);
	}

	constexpr int num_trials_per_game = 10;
	constexpr int multi_pv = 1;

	int game_index = 0;
	//std::string directory_path = "C:\\shogi\\dr2_kifu_csa";
	std::string directory_path = "D:\\hnoda\\dr2_kifu_csa";
	for (const auto& directory_entry : std::filesystem::directory_iterator(directory_path)) {
		const auto& path = directory_entry.path();
		const auto& filename = path.filename().string();

		Color target_color;
		if (filename.find("_" + static_cast<std::string>(Options["Target"]) + "_") != std::string::npos) {
			target_color = BLACK;
		}
		else if (filename.find("_" + static_cast<std::string>(Options["Target"]) + "-") != std::string::npos) {
			target_color = WHITE;
		}
		else {
			continue;
		}

		int day;
		if (filename.find("+dr2prdy1-") != std::string::npos) {
			day = 1;
		}
		else {
			day = 2;
		}

		for (int trial = 0; trial < num_trials_per_game; ++trial) {
			std::ifstream ifs(path);
			assert(ifs);

			std::string line;
			while (std::getline(ifs, line) && line != "+")
				;

			// 局面を初期化する。
			StateListPtr states(new StateList(1));
			{
				std::istringstream iss("startpos");
				position_cmd(pos, iss, states);
			}
			is_ready();

			Color color = BLACK;
			std::string position_command = "startpos";
			while (std::getline(ifs, line) && line.find("%") != 0) {
				auto csa_move_str = line.substr(1);
				auto move = CSA::to_move(pos, csa_move_str);

				std::string time_str;
				std::getline(ifs, time_str);

				if (target_color == color) {
					int progress_type = static_cast<int>(progress->Estimate(pos) * 3.0);

					// goコマンドを実行する
					{
						std::istringstream iss("go byoyomi 5000");
						go_cmd(pos, iss, states);
					}

					// goコマンドを待機する
					Threads.main()->wait_for_search_finished();

					// 棋譜の指し手が読み筋に含まれているかどうか調べる。
					bool found = false;
					const auto& root_moves = Threads.main()->rootMoves;
					for (int pv_index = 0; !found && pv_index < multi_pv && pv_index < root_moves.size(); ++pv_index) {
						const auto& pv = root_moves[pv_index].pv;
						if (pv.empty()) {
							continue;
						}

						found = (pv[0] == move);
					}

					std::printf("%d,%d,%d,%d,%d,%d\n", day, game_index, trial, pos.game_ply(), progress_type, found ? 1 : 0);
					std::fflush(nullptr);
				}

				if (pos.game_ply() == 1) {
					position_command += " moves";
				}
				position_command += " ";
				position_command += to_usi_string(move);

				{
					std::istringstream iss(position_command);
					position_cmd(pos, iss, states);
				}

				color = ~color;
			}
		}

		++game_index;
	}

	std::cout << "done" << std::endl;
}
