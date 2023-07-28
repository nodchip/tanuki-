#include "tanuki_kifu_filter.h"
#include "config.h"

#ifdef EVAL_LEARN

#include "thread.h"

using USI::Option;
using USI::OptionsMap;

namespace {
	constexpr const char* kOptionFilterBishopExchangeInputFileName = "FilterBishopExchangeInputFileName";
	constexpr const char* kOptionFilterBishopExchangeOutputFileNameInclude = "FilterBishopExchangeOutputFileNameInclude";
	constexpr const char* kOptionFilterBishopExchangeOutputFileNameExclude = "FilterBishopExchangeOutputFileNameExclude";
}

void Tanuki::InitializeFilter(USI::OptionsMap& o) {
	o[kOptionFilterBishopExchangeInputFileName] << Option("");
	o[kOptionFilterBishopExchangeOutputFileNameInclude] << Option("");
	o[kOptionFilterBishopExchangeOutputFileNameExclude] << Option("");
}

void Tanuki::FilterBishopExchange()
{
	std::string input_file_name = Options[kOptionFilterBishopExchangeInputFileName];
	std::string output_file_name_include = Options[kOptionFilterBishopExchangeOutputFileNameInclude];
	std::string output_file_name_exclude = Options[kOptionFilterBishopExchangeOutputFileNameExclude];

	if (input_file_name == "") {
		sync_cout << "FilterBishopExchangeInputFileName is not specified." << sync_endl;
		std::exit(1);
	}
	else if (output_file_name_include == "") {
		sync_cout << "FilterBishopExchangeOutputFileNameInclude is not specified." << sync_endl;
		std::exit(1);
	}
	else if (output_file_name_exclude == "") {
		sync_cout << "FilterBishopExchangeOutputFileNameExclude is not specified." << sync_endl;
		std::exit(1);
	}

	std::ifstream ifs(input_file_name);
	std::ofstream ofs_include(output_file_name_include);
	std::ofstream ofs_exclude(output_file_name_exclude);

	std::string line;
	while (std::getline(ifs, line)) {
		auto& position = Threads[0]->rootPos;
		std::vector<StateInfo> state_info(MAX_PLY * 2);
		position.set_hirate(&state_info[0], Threads.main());

		bool bishop_exchange = false;
		std::istringstream iss(line);
		std::string token;
		while (!bishop_exchange && position.game_ply() <= 20 && iss >> token) {
			if (token == "startpos" || token == "moves") {
				continue;
			}

			auto move = USI::to_move(position, token);
			if (move == MOVE_RESIGN || move == MOVE_WIN || move == MOVE_NULL || MOVE_NONE) {
				break;
			}

			position.do_move(move, state_info[position.game_ply()]);

			auto black_hand = position.hand_of<BLACK>();
			auto white_hand = position.hand_of<WHITE>();
			bishop_exchange = hand_count(black_hand, BISHOP) > 0 && hand_count(white_hand, BISHOP) > 0;
		}

		if (bishop_exchange) {
			ofs_include << line << std::endl;
		}
		else {
			ofs_exclude << line << std::endl;
		}
	}
}

#endif
