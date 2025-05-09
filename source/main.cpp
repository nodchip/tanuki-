//#include <iostream>
//#include "bitboard.h"
//#include "position.h"
#include "search.h"
#include "thread.h"
#include "tt.h"
#include "usi.h"
#include "misc.h"

// ファイルの中身を出力する。
void print_file(const std::string& path)
{
	SystemIO::TextReader reader;
	if (reader.Open(path).is_not_ok())
		return;

	std::string line;
	while (reader.ReadLine(line).is_ok())
		sync_cout << line << sync_endl;
}

// ----------------------------------------
//  main()
// ----------------------------------------

int main(int argc, char* argv[])
{
	// 起動時に説明書きを出力。
	print_file("startup_info.txt");

	// --- 全体的な初期化

	Bitboards::init();
	Position::init();

	USI usi(argc,argv);

	Eval::init();

#if !defined(__EMSCRIPTEN__)
	// USIコマンドの応答部

	usi.loop(argc, argv);

#else
	// yaneuraOu.wasm
	// ここでループしてしまうと、ブラウザのメインスレッドがブロックされてしまうため、コメントアウト
#endif

	return 0;
}
