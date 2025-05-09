#ifndef USI_H_INCLUDED
#define USI_H_INCLUDED

#include <cstddef>
#include <iosfwd>
#include <map>
#include <string>

#include "position.h"
#include "thread.h"
#include "tt.h"
#include "types.h"
#include "usi_option.h"

namespace Search {
	class Worker;
}
class Learner;

// --------------------
//     USI関連
// --------------------

class USI
{
public:
	USI(int argc, char** argv);

	// USIメッセージ応答部(起動時に、各種初期化のあとに呼び出される)
	void loop(int argc, char* argv[]);

#if defined(USE_PIECE_VALUE)

	// Valueをcp(centi-pawn)に変換する。
	static int to_cp(Value v);

	// cpからValueへ。⇑の逆変換。
	static Value cp_to_value(int v);

	// USIプロトコルの形式でValue型を出力する。
	// 歩が100になるように正規化するので、operator <<(Value)をこういう仕様にすると
	// 実際の値と異なる表示になりデバッグがしにくくなるから、そうはしていない。
	// USE_PIECE_VALUEが定義されていない時は正規化しようがないのでこの関数は呼び出せない。
	static std::string value(Value v);

#endif

	// Square型をUSI文字列に変換する
	static std::string square(Square s);

	// 指し手をUSI文字列に変換する。
	static std::string move(Move   m /*, bool chess960*/);
	static std::string move(Move16 m /*, bool chess960*/);

	// 読み筋をUSI文字列化して返す。
	// " 7g7f 8c8d" のように返る。
	static std::string move(const std::vector<Move>& moves);

	static std::string pv(
		const Search::Worker& workerThread, Position& pos, TimePoint elapsed,
		uint64_t nodesSearched, int hashfull, Search::LimitsType& limits,
		TranspositionTable& tt);

	// 局面posとUSIプロトコルによる指し手を与えて
	// もし可能なら等価で合法な指し手を返す。
	// 合法でないときはMOVE_NONEを返す。(この時、エラーである旨を出力する。)
	// "resign"に対してはMOVE_RESIGNを返す。
	// Stockfishでは第二引数にconstがついていないが、これはつけておく。
	// 32bit Moveが返る。(Move16ではないことに注意)
	static Move to_move(const Position& pos, const std::string& str);

	// -- 以下、やねうら王、独自拡張。

	// 合法かのテストはせずにともかく変換する版。
	// 返ってくるのは16bitのMoveなので、これを32bitのMoveに変換するには
	// Position::move16_to_move()を呼び出す必要がある。
	// Stockfishにはない関数だが、高速化を要求されるところで欲しいので追加する。
	static Move16 to_move16(const std::string& str);

	OptionsMap options;

	static std::vector<std::string> ekr_rules;

private:
	TranspositionTable tt;
	ThreadPool         threads;
	CommandLine        cli;

	void go(const Position& pos, std::istringstream& is, StateListPtr& states, bool ignore_ponder = false);
	void bench(Position& current, std::istringstream& is);

	// positionコマンドのparserを呼び出したいことがあるので外部から呼び出せるようにしておく。
	// 使い方はbenchコマンド(benchmark.cpp)のコードを見てほしい。
	void position(Position& pos, std::istringstream& is, StateListPtr& states);

	void setoption(std::istringstream& is);


	// === やねうら王独自実装 ===

	void cmdexec(Position& pos, StateListPtr& states, std::string& cmd);
	bool parse_ponderhit(std::istringstream& is);

#if defined(USE_GAMEOVER_HANDLER) || defined(YANEURAOU_ENGINE_DEEP)
	void USI::gameover(const string& cmd);
#endif

	// USIの"isready"コマンドが呼び出されたときの処理。このときに評価関数の読み込みなどを行なう。
	// benchmarkコマンドのハンドラなどで"isready"が来ていないときに評価関数を読み込ませたいときに用いる。
	// skipCorruptCheck == trueのときは評価関数の2度目の読み込みのときのcheck sumによるメモリ破損チェックを省略する。
	// ※　この関数は、Stockfishにはないがないと不便なので追加しておく。
	static void isready(bool skipCorruptCheck = false);
	void isready(Position& pos, StateListPtr& states);

	// USIに追加オプションを設定したいときは、この関数を定義すること。
	// USI::init()のなかからコールバックされる。
	void extra_option(OptionsMap& o);

	// 評価関数を読み込んだかのフラグ。これはevaldirの変更にともなってfalseにする。
	static bool load_eval_finished; // = false;

#if defined (USE_ENTERING_KING_WIN)
	// 入玉ルール文字列をEnteringKingRule型に変換する。
	EnteringKingRule to_entering_king_rule(const std::string& rule);
#endif

	void build_option(const std::string& line);

	// エンジンオプションをコンパイル時に設定する機能
	// "ENGINE_OPTIONS"で指定した内容を設定する。
	// 例) #define ENGINE_OPTIONS "FV_SCALE=24;BookFile=no_book"
	void set_engine_options(const std::string& options_string);

	// エンジンオプションのoverrideのためにファイルから設定を読み込む。
	// 1) これは起動時に"engine_options.txt"という設定ファイルを読み込むのに用いる。
	// 2) "isready"応答に対して、EvalDirのなかにある"eval_options.txt"という設定ファイルを読み込むのにも用いる。
	void read_engine_options(const std::string& filename);

	void getoption(std::istringstream& is);

	void qsearch(Position& pos);

	void search(Position& pos, std::istringstream& is);

	void test(Position& pos, std::istringstream& is);

	// namespace USI内のUnitTest。
	void UnitTest(Test::UnitTester& tester);

	static std::string last_eval_dir;
	static u64 eval_sum;

#if defined(EVAL_LEARN)
	friend class Learner;
#endif
};


#endif // #ifndef USI_H_INCLUDED
