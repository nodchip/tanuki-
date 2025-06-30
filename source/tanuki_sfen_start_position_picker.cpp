#ifdef EVAL_LEARN
#include "tanuki_sfen_start_position_picker.h"

#include "thread.h"
#include "usi.h"

using Tanuki::SfenStartPositionPicker;

namespace {
	constexpr const char* kOptionGeneratorStartposFileName = "GeneratorStartposFileName";
	constexpr const char* kOptionGeneratorStartPositionMaxPlay = "GeneratorStartPositionMaxPlay";
}

bool SfenStartPositionPicker::Open()
{
	start_positions_.clear();

	// 定跡ファイル(というか単なる棋譜ファイル)の読み込み
	std::string book_file_name = Options[kOptionGeneratorStartposFileName];
	std::ifstream fs_book;
	fs_book.open(book_file_name);

	int start_position_max_play = static_cast<int>(Options[kOptionGeneratorStartPositionMaxPlay]);

	if (!fs_book.is_open()) {
		sync_cout << "Error! : can't read " << book_file_name << sync_endl;
		return false;
	}

	sync_cout << "Reading " << book_file_name << sync_endl;
	std::string line;
	int line_index = 0;
	std::vector<StateInfo> state_info(4096);
	while (!fs_book.eof()) {
		Thread& thread = *Threads[0];
		Position& pos = thread.rootPos;

		std::getline(fs_book, line);
		std::istringstream is(line);
		std::string token;

		is >> token;
		if (token == "startpos") {
			pos.set_hirate(&state_info[0], &thread);
			// movesを読み飛ばす。
			is >> token;
		}
		else if (token == "sfen") {
			std::string sfen;
			// movesより前までをスペース区切りで結合する。
			while (is >> token && token != "moves") {
				if (!sfen.empty()) {
					sfen += " ";
				}
				sfen += token;
			}
			pos.set(sfen, &state_info[0], &thread);
		}

		while (pos.game_ply() <= start_position_max_play) {
			if (!(is >> token)) {
				break;
			}
			if (token == "startpos" || token == "moves") continue;

			Move m = USI::to_move(pos, token);
			if (!is_ok(m) || !pos.legal(m)) {
				//  sync_cout << "Error book.sfen , line = " << book_number << " , moves = " <<
				//  token << endl << rootPos << sync_endl;
				// →　エラー扱いはしない。
				break;
			}

			pos.do_move(m, state_info[pos.game_ply()]);
			start_positions_.push_back(pos.sfen());
		}

		if ((++line_index % 1000) == 0) std::cout << ".";
	}
	std::cout << std::endl;
	sync_cout << "Number of lines: " << line_index << sync_endl;
	sync_cout << "Number of start positions: " << start_positions_.size() << sync_endl;

	// 開始局面集をシャッフルする。
	std::random_device rd;
	std::mt19937_64 mt(rd());
	std::shuffle(start_positions_.begin(), start_positions_.end(), mt);

	// 開始位置を初期化する。
	start_positions_iterator_ = start_positions_.begin();
	ASSERT_LV3(start_positions_iterator_ != start_positions_.end())
		return true;
}

void SfenStartPositionPicker::Pick(Position& position, StateInfo*& state_info, Thread& thread)
{
	std::lock_guard<std::mutex> lock(mutex_);
	position.set(*start_positions_iterator_++, state_info++, &thread);
	if (start_positions_iterator_ == start_positions_.end()) {
		start_positions_iterator_ = start_positions_.begin();
	}
}
#endif // EVAL_LEARN
