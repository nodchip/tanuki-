#include "usi.h"

#include "extra/bitop.h"
#include "misc.h"
#include "position.h"
#include "search.h"
#include "testcmd/unit_test.h"
#include "thread.h"
#include "types.h"

#if !defined(YANEURAOU_ENGINE_DEEP)
#include "tt.h"
#endif

#if defined (ENABLE_MAKEBOOK_CMD)
#include "book/book.h"
#endif

#if defined (EVAL_LEARN)
#include "learn/learn.h"
#endif

#if defined(__EMSCRIPTEN__)
// yaneuraou.wasm
#include <emscripten.h>
#endif

#include <sstream>
#include <queue>

using namespace std;

// 前回のOptions["EvalDir"]
std::string USI::last_eval_dir;

// 入玉ルールのUSI文字列
std::vector<std::string> USI::ekr_rules = { "NoEnteringKing", "CSARule24" , "CSARule24H" , "CSARule27" , "CSARule27H", "TryRule" };

// Normalizes the internal value as reported by evaluate or search
// to the UCI centipawn result used in output. This value is derived from
// the win_rate_model() such that Stockfish outputs an advantage of
// "100 centipawns" for a position if the engine has a 50% probability to win
// from this position in self-play at fishtest LTC time control.

// evaluateまたはsearchによって報告される内部値をUSIの出力で使用されるUSIのcenti-pawnの値に正規化します
// この値はwin_rate_model()から派生しており、
// Stockfishがこのポジションから自己対局で50%の確率で勝利する場合、
// "100セントポーン"の利点を出力します。
// これは、fishtest LTCタイムコントロールでの自己対局においてです。

#if defined(USE_PIECE_VALUE)
	// → やねうら王の場合、PawnValue = 90なので Value = 90なら 100として出力する必要がある。
	// Stockfish 16ではこの値は328になっている。
constexpr int NormalizeToPawnValue = Eval::PawnValue;
#endif

USI::USI(int argc, char** argv) :
	cli(argc, argv) {
#if !defined(__EMSCRIPTEN__)
	// Hash上限。32bitモードなら2GB、64bitモードなら33TB
	constexpr int MaxHashMB = Is64Bit ? 33554432 : 2048;
#else
	// yaneuraou.wasm
	// メモリの調整
	// stockfish.wasmの数値を基本的に使用している
	constexpr int MaxHashMB = 2048;
#endif

	// 並列探索するときのスレッド数
	// CPUの搭載コア数をデフォルトとすべきかも知れないが余計なお世話のような気もするのでしていない。

#if !defined(YANEURAOU_ENGINE_DEEP)

		// ※　やねうら王独自改良
		// スレッド数の変更やUSI_Hashのメモリ確保をそのハンドラでやってしまうと、
		// そのあとLargePageEnableを送られても困ることになる。
		// ゆえにこれらは、"isready"に対する応答で行うことにする。
		// そもそもで言うとsetoptionに対してそんなに時間のかかることをするとGUI側がtimeoutになる懸念もある。
		// Stockfishもこうすべきだと思う。

#if !defined(__EMSCRIPTEN__)
	options["Threads"] << Option(4, 1, 1024, []([[maybe_unused]] const Option& o) { /* on_threads(o); */ });
#else
		// yaneuraou.wasm
		// スレッド数などの調整
		// stockfish.wasmの数値を基本的に使用している
	options["Threads"] << Option(1, 1, 32, []([[maybe_unused]] const Option& o) { /* on_threads(o); */ });
#endif
#endif

#if !defined(TANUKI_MATE_ENGINE) && !defined(YANEURAOU_MATE_ENGINE)
	// 置換表のサイズ。[MB]で指定。
	options["USI_Hash"] << Option(1024, 1, MaxHashMB, []([[maybe_unused]] const Option& o) { /* on_hash_size(o); */ });

#if defined(USE_EVAL_HASH)
	// 評価値用のcacheサイズ。[MB]で指定。

#if defined(FOR_TOURNAMENT)
		// トーナメント用は少し大きなサイズ
	options["EvalHash"] << Option(1024, 1, MaxHashMB, [](const Option& o) { Eval::EvalHash_Resize(o); });
#else
	options["EvalHash"] << Option(128, 1, MaxHashMB, [](const Option& o) { Eval::EvalHash_Resize(o); });
#endif // defined(FOR_TOURNAMENT)
#endif // defined(USE_EVAL_HASH)

	// Stockfishには、探索部を初期化するエンジンオプションがあるが使わないので未サポートとする。
	//o["Clear Hash"]            << Option(on_clear_hash);

	// ponderの有無
	options["USI_Ponder"] << Option(false);

	// 確率的ponder , defaultでfalseにしとかないと、読み筋の表示がおかしくなって、初心者混乱する。
	options["Stochastic_Ponder"] << Option(false);

	// その局面での上位N個の候補手を調べる機能
	// ⇨　これMAX_MOVESで十分。
	options["MultiPV"] << Option(1, 1, MAX_MOVES);

	// 指し手がGUIに届くまでの時間。
#if defined(YANEURAOU_ENGINE_DEEP)
		// GPUからの結果を待っている時間も込みなので少し上げておく。
	int time_margin = 400;
#else
	int time_margin = 120;
#endif

	// ネットワークの平均遅延時間[ms]
	// この時間だけ早めに指せばだいたい間に合う。
	// 切れ負けの瞬間は、NetworkDelayのほうなので大丈夫。
	options["NetworkDelay"] << Option(time_margin, 0, 10000);

	// ネットワークの最大遅延時間[ms]
	// 切れ負けの瞬間だけはこの時間だけ早めに指す。
	// 1.2秒ほど早く指さないとfloodgateで切れ負けしかねない。
	options["NetworkDelay2"] << Option(time_margin + 1000, 0, 10000);

	// 最小思考時間[ms]
	options["MinimumThinkingTime"] << Option(2000, 1000, 100000);

	// 切れ負けのときの思考時間を調整する。序盤重視率。百分率になっている。
	// 例えば200を指定すると本来の最適時間の200%(2倍)思考するようになる。
	// 対人のときに短めに設定して強制的に早指しにすることが出来る。
	options["SlowMover"] << Option(100, 1, 1000);

	// 引き分けまでの最大手数。256手ルールのときに256を設定すると良い。0なら無制限。
	options["MaxMovesToDraw"] << Option(0, 0, 100000);

	// 探索深さ制限。0なら無制限。
	options["DepthLimit"] << Option(0, 0, int_max);

	// 探索ノード制限。0なら無制限。
	options["NodesLimit"] << Option(0, 0, int64_max);

	// 評価関数フォルダ。これを変更したとき、評価関数を次のisreadyタイミングで読み直す必要がある。
#if defined(EVAL_EMBEDDING)
	const char* default_eval_dir = "<internal>";
#elif !defined(__EMSCRIPTEN__)
	const char* default_eval_dir = "eval";
#else
		// WASM
	const char* default_eval_dir = ".";
#endif
	last_eval_dir = default_eval_dir;
	options["EvalDir"] << Option(default_eval_dir, [](const Option& o) {
		if (last_eval_dir != string(o))
		{
			// 評価関数フォルダ名の変更に際して、評価関数ファイルの読み込みフラグをクリアする。
			last_eval_dir = string(o);
			load_eval_finished = false;
		}
		});
#if defined(__EMSCRIPTEN__) && defined(EVAL_NNUE)
	// WASM NNUE
	const char* default_eval_file = "nn.bin";
	last_eval_file = default_eval_file;
	options["EvalFile"] << Option(default_eval_file, [](const USI::Option& o) {
		if (last_eval_file != string(o))
		{
			// 評価関数ファイル名の変更に際して、評価関数ファイルの読み込みフラグをクリアする。
			last_eval_file = string(o);
			load_eval_finished = false;
		}
		});
#endif

#else

	// TANUKI_MATE_ENGINEのとき
options["USI_Hash"] << Option(std::min(4096, MaxHashMB), 1, MaxHashMB);

#endif // !defined(TANUKI_MATE_ENGINE) && !defined(YANEURAOU_MATE_ENGINE)

	// cin/coutの入出力をファイルにリダイレクトする
	options["WriteDebugLog"] << Option("", [](const Option& o) { start_logger(o); });

	// 読みの各局面ですべての合法手を生成する
	// (普通、歩の2段目での不成などは指し手自体を生成しないが、
	// これのせいで不成が必要な詰みが絡む問題が解けないことがあるので、このオプションを用意した。)
#if defined(TANUKI_MATE_ENGINE) || defined(YANEURAOU_MATE_ENGINE)
		// 詰将棋エンジンではデフォルトでオン。
	options["GenerateAllLegalMoves"] << Option(true);
#else
		// 通常探索エンジンではデフォルトでオフ。
	options["GenerateAllLegalMoves"] << Option(false);
#endif

#if defined (USE_ENTERING_KING_WIN)
	// 入玉ルール
	options["EnteringKingRule"] << Option(ekr_rules, ekr_rules[EKR_27_POINT]);
#endif

#if defined (USE_SHARED_MEMORY_IN_EVAL) && defined(_WIN32) && \
	 (defined(EVAL_KPPT) || defined(EVAL_KPP_KKPT) )
	// 評価関数パラメーターを共有するか。
	// デフォルトで有効に変更。(V4.90～)
	options["EvalShare"] << Option(true);
#endif

#if defined(EVAL_LEARN)
	// isreadyタイミングで評価関数を読み込まれると、新しい評価関数の変換のために
	// test evalconvertコマンドを叩きたいのに、その新しい評価関数がないがために
	// このコマンドの実行前に異常終了してしまう。
	// そこでこの隠しオプションでisready時の評価関数の読み込みを抑制して、
	// test evalconvertコマンドを叩く。
	options["SkipLoadingEval"] << Option(false);
#endif

#if defined(_WIN64)
	// LargePageを有効化するか。
	// これを無効化できないと自己対局の時に片側のエンジンだけがLargePageを使うことがあり、
	// 不公平になるため、無効化する方法が必要であった。
	options["LargePageEnable"] << Option(true);
#endif

	// 各エンジンがOptionを追加したいだろうから、コールバックする。
	USI::extra_option(options);

#if defined(ENGINE_OPTIONS)
	const string opt = ENGINE_OPTIONS;
	set_engine_options(opt);
#endif

	// カレントフォルダに"engine_options.txt"があればそれをオプションとしてOptions[]の値をオーバーライドする機能。
	read_engine_options("engine_options.txt");

	threads.set({ options, threads, tt });
}

// USI応答部ループ
void USI::loop(int argc, char* argv[])
{
	// 探索開始局面(root)を格納するPositionクラス
	// "position"コマンドで設定された局面が格納されている。
	Position pos;

	string cmd, token;

	// 局面を遡るためのStateInfoのlist。
	StateListPtr states(new StateList(1));

	std_input.parse_args(argc, argv);

	// このファイルがあれば、この内容を実行してやる。
	const string startup = "startup.txt";
	vector<string> lines;
	if (SystemIO::ReadAllLines(startup, lines).is_ok())
	{
		for (auto& line : lines)
			std_input.push(line);
	}

	do
	{
		cmd = std_input.input();
		cmdexec(pos, states, cmd);

		// quit検知
		istringstream is(cmd);
		is >> skipws >> token;
	} while (token != "quit");

	// quitが来た時点ではまだ探索中かも知れないのでmain threadの停止を待つ。
	threads.main_thread()->wait_for_search_finished();
}

// --------------------
// 　　USI応答部
// --------------------

// USI応答部本体
void USI::cmdexec(Position& pos, StateListPtr& states, string& cmd)
{
	string token;

	{
		istringstream is(cmd);

		token.clear(); // getlineが空を返したときのためのクリア
		is >> skipws >> token;

		if (token == "quit" || token == "stop" || token == "gameover")
		{
			// USIプロトコルにはUCIプロトコルから、
			// gameover win | lose | draw
			// が追加されているが、stopと同じ扱いをして良いと思う。
			// これハンドルしておかないとponderが停止しなくて困る。
			// gameoverに対してbestmoveは返すべきではないのかも知れないが、
			// それを言えばstopにだって…。

#if defined(USE_GAMEOVER_HANDLER) || defined(YANEURAOU_ENGINE_DEEP)

			if (token == "gameover")
				// "gameover"コマンドに対するハンドラを呼び出したいのか？
				gameover_handler(cmd);

#endif

			// "go infinite" , "go ponder"などで思考を終えて寝てるかも知れないが、
			// そいつらはThreads.stopを待っているので問題ない。
			threads.stop = true;

		}
		else if (token == "ponderhit")
		{
			if (options["Stochastic_Ponder"])
			{
				// Stochastic Ponder hit

				// まず探索スレッドを停止させる。
				// ただしこの時にbestmoveを返してはならないので、これはSearch::Limits.silentで抑制する。
				auto org = threads.main_thread()->worker->limits.silent;
				for (const auto& thread : threads) {
					thread->worker->limits.silent = true;
				}
				threads.stop = true;
				// 終了を待機しないとsilentの解除ができない。
				threads.main_thread()->wait_for_search_finished();
				for (const auto& thread : threads) {
					thread->worker->limits.silent = org;
				}

				// 前回と同様のgoコマンドをそのまま送る。ただし"ponder"の文字は無視する。
				// last_go_cmd_stringには先頭に"go"の文字があるが、それはgo_cmdのなかで無視されるので気にしなくて良い。
				istringstream iss(threads.main_manager()->last_go_cmd_string);
				go(pos, iss, states, true);
			}
			else {
				// ponderhitに追加パラメーターがあるか？(USI拡張プロトコル)

#if defined(USE_TIME_MANAGEMENT)
				bool token_processed = parse_ponderhit(is);
				// 追加パラメーターを処理したなら今回の思考時間を再計算する。
				if (token_processed)
					threads.main_manager()->tm.reinit(options);
#endif

				// 通常のponder
				threads.main_manager()->tm.reset_for_ponderhit();     // ponderhitから計測しなおすべきである。
				threads.main_manager()->ponder = false; // 通常探索に切り替える。
			}
		}

		// 起動時いきなりこれが飛んでくるので速攻応答しないとタイムアウトになる。
		else if (token == "usi")
			sync_cout << engine_info() << options << "usiok" << sync_endl;

		// オプションを設定する
		else if (token == "setoption") setoption(is);

		// 与えられた局面について思考するコマンド
		else if (token == "go") {
			threads.main_manager()->last_go_cmd_string = cmd;       // Stochastic_Ponderで使うので保存しておく。
			go(pos, is, states);
		}

		// (思考などに使うための)開始局面(root)を設定する
		else if (token == "position") {
			threads.main_manager()->last_position_cmd_string = cmd; // 保存しておく。
			position(pos, is, states);
		}

		// "usinewgame"はゲーム中にsetoptionなどを送らないことを宣言するためのものだが、
		// 我々はこれに関知しないので単に無視すれば良い。
		// やねうら王では、時間のかかる初期化はisreadyの応答でやっている。
		// Stockfishでは、Search::clear() (時間のかかる処理)をここで呼び出しているようだが。
		// そもそもで言うと、"usinewgame"に対してはエンジン側は何ら応答を返さないので、
		// GUI側は、エンジン側が処理中なのかどうかが判断できない。
		// なのでここで長い時間のかかる処理はすべきではないと思うのだが。
		else if (token == "usinewgame") return;

		// 思考エンジンの準備が出来たかの確認
		else if (token == "isready") isready(pos, states);

		// 以下、デバッグのためのカスタムコマンド(非USIコマンド)
		// 探索中には使わないようにすべし。

#if defined(USER_ENGINE)
		// ユーザーによる実験用コマンド。user.cppのuser()が呼び出される。
		else if (token == "user") user_test(pos, is);
#endif

		// ベンチコマンド(これは常に使える)
		else if (token == "bench") bench(pos, is);

		// 現在の局面を表示する。(デバッグ用)
		else if (token == "d") cout << pos << endl;

		// USI Commands from File
		else if (token == "f") {
			string filename = "";
			is >> filename;
			if (!filename.empty())
			{
				filename += ".txt";
				sync_cout << "USI Commands from File = " << filename << sync_endl;
				vector<string> lines;

				SystemIO::ReadAllLines(filename, lines);
				for (auto& line : lines)
					std_input.push(line);
			}
		}

		// 現在の局面について評価関数を呼び出して、その値を返す。
		else if (token == "eval") cout << "eval = " << Eval::compute_eval(pos) << endl;
		else if (token == "evalstat") Eval::print_eval_stat(pos);

		// この実行ファイルをコンパイルしたコンパイラの情報を出力する。
		else if (token == "compiler") sync_cout << compiler_info() << sync_endl;

		// -- 以下、やねうら王独自拡張のカスタムコマンド

		// config.hで設定した値などについて出力する。
		else if (token == "config") sync_cout << config_info() << sync_endl;

		// オプションを取得する(USI独自拡張)
		else if (token == "getoption") getoption(is);

		// 指し手生成祭りの局面をセットする。
		else if (token == "matsuri") pos.set("l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w GR5pnsg 1", &states->back());

		// "position sfen"の略。
		else if (token == "sfen") position(pos, is, states);

		// ログファイルの書き出しのon
		// 備考)
		// Stockfishの方は、エンジンオプションでログの出力ファイル名を指定できるのだが、
		// ログ自体はホスト側で記録することが多いので、ファイル名は固定でいいや…。
		else if (token == "log") start_logger("io_log.txt");

#if defined(EVAL_LEARN)
		// テスト用にqsearch(),search()を直接呼ぶコマンド
		else if (token == "qsearch") qsearch(pos);
		else if (token == "search") search(pos, is);
#endif

		// この局面での指し手をすべて出力
		else if (token == "moves") {
			for (auto m : MoveList<LEGAL_ALL>(pos))
				cout << Move(m) << ' ';
			cout << endl;
		}

		// この局面の手番側がどちらであるかを返す。BLACK or WHITE
		else if (token == "side") cout << (pos.side_to_move() == BLACK ? "black" : "white") << endl;

		// この局面が詰んでいるかの判定
		else if (token == "mated") cout << pos.is_mated() << endl;

		// この局面のhash keyの値を出力
		else if (token == "key") cout << hex << pos.state()->hash_key() << dec << endl;

		// 探索の終了を待機するコマンド("stop"は送らずに。goコマンドの終了を待機できて便利。)
		else if (token == "wait") threads.main_thread()->wait_for_search_finished();

		// 一定時間待機するコマンド。("quit"の前に一定時間待ちたい時などに用いる。sleep 1000 == 1秒待つ)
		else if (token == "sleep") { u64 ms; is >> ms; Tools::sleep(ms); }

#if defined(MATE_1PLY) && defined(LONG_EFFECT_LIBRARY)
		// この局面での1手詰め判定
		else if (token == "mate1") cout << pos.mate1ply() << endl;
#endif

#if defined (ENABLE_TEST_CMD)
		// テストコマンド
		else if (token == "test") test(pos, is);
#endif

		// UnitTest
		else if (token == "unittest") Test::UnitTest(pos, is);

#if defined (ENABLE_MAKEBOOK_CMD) && (defined(EVAL_LEARN) || defined(YANEURAOU_ENGINE_DEEP))
		// 定跡を作るコマンド
		else if (token == "makebook") Book::makebook_cmd(pos, is);
#endif

#if defined (EVAL_LEARN)
		else if (token == "gensfen") Learner::gen_sfen(pos, is);
		else if (token == "learn") Learner::learn(pos, is);

#if defined (GENSFEN2019)
		// 開発中の教師局面生成コマンド
		else if (token == "gensfen2019") Learner::gen_sfen2019(pos, is);
#endif

#endif

#if defined(USE_YO_CLUSTER)
#if defined(YANEURAOU_ENGINE_DEEP) || defined(YANEURAOU_ENGINE_NNUE)
		else if (token == "cluster")
			// cluster時のUSIメッセージの処理ループ
			YaneuraouTheCluster::cluster_usi_loop(pos, is);
#endif
#endif

		else
		{
			//    簡略表現として、
			//> threads 1
			//      のように指定したとき、
			//> setoption name Threads value 1
			//      と等価なようにしておく。

			if (!token.empty())
			{
				string value;
				is >> value;

				for (auto& o : options)
				{
					// 大文字、小文字を無視して比較。
					if (!StringExtension::stricmp(token, o.first))
					{
						options[o.first] = value;
						sync_cout << "Options[" << o.first << "] = " << value << sync_endl;

						goto OPTION_FOUND;
					}
				}
				sync_cout << "No such option: " << token << sync_endl;
			OPTION_FOUND:;
			}
		}
	}
}

// Called when the engine receives the "go" UCI command. The function sets the
// thinking time and other parameters from the input string then stars with a search

// go()は、思考エンジンがUSIコマンドの"go"を受け取ったときに呼び出される。
// この関数は、入力文字列から思考時間とその他のパラメーターをセットし、探索を開始する。
// ignore_ponder : これがtrueなら、"ponder"という文字を無視する。
void USI::go(const Position& pos, istringstream& is, StateListPtr& states, bool ignore_ponder) {

	// "isready"コマンド受信前に"go"コマンドが呼び出されている。
	if (!USI::load_eval_finished)
	{
		sync_cout << "info string Error! go cmd before isready cmd." << sync_endl;
		return;
	}

	// Position::update_entering_point()がlimitsを書き換えている可能性があるため、
	// 前回の値からの差分をstart_thinking()に渡すようにする。
	Search::LimitsType limits;
	string token;
	bool ponderMode = false;

	auto main_thread = threads.main_manager();

	if (!states)
	{
		// 前回から"position"コマンドを処理せずに再度goが呼び出された。
		// 前回、ponderでStochastic Ponderのために局面を壊してしまっている可能性があるので復元しておく。
		// (これがStochastic Ponderの一番簡単な実装)
		// Stochastic Ponderのために局面を2手前に戻して、そのあと現在の局面に対するコマンド("d"など)を実行すると
		// それは2手前の局面が表示されるが、それは仕様であるものとする。(これを修正するとプログラムのフローが複雑になる)
		istringstream iss(main_thread->last_position_cmd_string);
		iss >> token; // "position"
		position(*const_cast<Position*>(&pos), iss, states);
	}

	// 思考開始時刻の初期化。なるべく早い段階でこれをしておかないとサーバー時間との誤差が大きくなる。
	threads.main_manager()->tm.reset();

	// 終局(引き分け)になるまでの手数
	// 引き分けになるまでの手数。(Options["MaxMovesToDraw"]として与えられる。エンジンによってはこのオプションを持たないこともある。)
	// 0のときは制限なしだが、これをint_maxにすると残り手数を計算するときに桁があふれかねないので100000を設定。

	int max_game_ply = 0;
	if (options.count("MaxMovesToDraw"))
		max_game_ply = (int)options["MaxMovesToDraw"];

	// これ0の時、何らか設定しておかないと探索部でこの手数を超えた時に引き分け扱いにしてしまうので、無限大みたいな定数の設定が必要。
	limits.max_game_ply = (max_game_ply == 0) ? 100000 : max_game_ply;

#if defined (USE_ENTERING_KING_WIN)
	// 入玉ルール
	limits.enteringKingRule = to_entering_king_rule(options["EnteringKingRule"]);
#endif

	// すべての合法手を生成するのか
	limits.generate_all_legal_moves = options["GenerateAllLegalMoves"];

	// エンジンオプションによる探索制限(0なら無制限)
	// このあと、depthもしくはnodesが指定されていたら、その値で上書きされる。(この値は無視される)

	limits.depth = options.count("DepthLimit") ? (int)options["DepthLimit"] : 0;
	limits.nodes = options.count("NodesLimit") ? (u64)options["NodesLimit"] : 0;

	while (is >> token)
	{
		// 探索すべき指し手。(探索開始局面から特定の初手だけ探索させるとき)
		// これ、Stockfishのコードでこうなっているからそのままにしてあるが、
		// これを指定しても定跡の指し手としてはこれ以外を指したりする問題はある。
		// またふかうら王ではこのオプションをサポートしていない。
		// ゆえに、非対応扱いで考えて欲しい。
		if (token == "searchmoves")
			// 残りの指し手すべてをsearchMovesに突っ込む。
			while (is >> token)
				limits.searchmoves.push_back(USI::to_move(pos, token));

		// 先手、後手の残り時間。[ms]
		else if (token == "wtime")     is >> limits.time[WHITE];
		else if (token == "btime")     is >> limits.time[BLACK];

		// フィッシャールール時における時間
		else if (token == "winc")      is >> limits.inc[WHITE];
		else if (token == "binc")      is >> limits.inc[BLACK];

		// "go rtime 100"だと100～300[ms]思考する。
		else if (token == "rtime")     is >> limits.rtime;

		// 秒読み設定。
		else if (token == "byoyomi") {
			TimePoint t = 0;
			is >> t;

			// USIプロトコルで送られてきた秒読み時間より少なめに思考する設定
			// ※　通信ラグがあるときに、ここで少なめに思考しないとタイムアップになる可能性があるので。

			// t = std::max(t - Options["ByoyomiMinus"], Time::point(0));

			// USIプロトコルでは、これが先手後手同じ値だと解釈する。
			limits.byoyomi[BLACK] = limits.byoyomi[WHITE] = t;
		}
		// この探索深さで探索を打ち切る
		else if (token == "depth")     is >> limits.depth;

		// この探索ノード数で探索を打ち切る
		else if (token == "nodes")     is >> limits.nodes;

		// 持ち時間固定(将棋だと対応しているGUIが無いかもしれないが..)
		else if (token == "movetime")  is >> limits.movetime;

		// 詰み探索。"UCI"プロトコルではこのあとには手数が入っており、その手数以内に詰むかどうかを判定するが、
		// "USI"プロトコルでは、ここは探索のための時間制限に変更となっている。
		else if (token == "mate") {
			is >> token;
			if (token == "infinite")
				limits.mate = INT32_MAX;
			else
				// USIプロトコルでは、UCIと異なり、ここは手数ではなく、探索に使う時間[ms]が指定されている。
				limits.mate = stoi(token);
		}

		// パフォーマンステスト(Stockfishにある、合法手N手で到達できる局面を求めるやつ)
		// このあとposition～goコマンドを使うとパフォーマンステストモードに突入し、ここで設定した手数で到達できる局面数を求める
		else if (token == "perft")		is >> limits.perft;

		// 時間無制限。
		else if (token == "infinite")	limits.infinite = 1;

		// ponderモードでの思考。
		else if (token == "ponder" && !ignore_ponder) {
			ponderMode = true;

			if (options["Stochastic_Ponder"] && main_thread->moves_from_game_root.size() >= 1)
			{
				// 1手前の局面(相手番)に戻して、ponderとして思考する。
				// Threads.main()->moves_from_game_root に保存されているので大丈夫。

				auto m = main_thread->moves_from_game_root.back();
				main_thread->moves_from_game_root.pop_back();
				const_cast<Position*>(&pos)->undo_move(m);
				states->pop_back();
				main_thread->position_is_dirty = true;
			}
		}

		// --- やねうら王独自拡張

		// "wait_stop"指定。
		else if (token == "wait_stop")
			limits.wait_stop = true;

#if defined(TANUKI_MATE_ENGINE)
		// MateEngineのデバッグ用コマンド: 詰将棋の特定の変化に対する解析を効率的に行うことが出来る。
		//	cf.https ://github.com/yaneurao/YaneuraOu/pull/115

		else if (token == "matedebug") {
			string token = "";
			Move16 m;
			limits.pv_check.clear();
			while (is >> token && (m = USI::to_move16(token)).to_u16() != MOVE_NONE) {
				limits.pv_check.push_back(m);
			}
		}
#endif

	}

	// goコマンド、デバッグ時に使うが、そのときに"go btime XXX wtime XXX byoyomi XXX"と毎回入力するのが面倒なので
	// デフォルトで1秒読み状態で呼び出されて欲しい。
	//if (limits.byoyomi[BLACK] == 0 && limits.inc[BLACK] == 0 && limits.time[BLACK] == 0 && limits.rtime == 0)
	//	limits.byoyomi[BLACK] = limits.byoyomi[WHITE] = 1000;

	// →　これやると、パラメーターなしで"go ponder"されて"ponderhit"したときに、byoyomi 1秒と錯覚する。

	// 検討モード用のPVを出力するのか。
	limits.consideration_mode = options["ConsiderationMode"];

	// fail low/highのときにPVを出力するかどうか。
	limits.outout_fail_lh_pv = options["OutputFailLHPV"];

	// PVの出力間隔[ms]
	// go infiniteはShogiGUIなどの検討モードで動作させていると考えられるので
	// この場合は、PVを毎回出力しないと読み筋が出力されないことがある。
	limits.pv_interval = (limits.infinite || limits.consideration_mode) ? 0 : (int)options["PvInterval"];

	pos.update_entering_point(limits);

	threads.start_thinking(options, pos, states, limits, ponderMode);
}

// "ponderhit"に"go"で使うようなwtime,btime,winc,binc,byoyomiが書けるような拡張。(やねうら王独自拡張。USI拡張プロトコル)
// 何かトークンを処理したらこの関数はtrueを返す。
bool USI::parse_ponderhit(istringstream& is)
{
	// 現在のSearch::Limitsに上書きしてしまう。
	auto limits = threads.main_thread()->worker->limits;
	string token;
	bool token_processed = false;

	while (is >> token)
	{
		// 何かトークンを処理したらこの関数はtrueを返す。
		token_processed = true;

		// 先手、後手の残り時間。[ms]
		if (token == "wtime")     is >> limits.time[WHITE];
		else if (token == "btime")     is >> limits.time[BLACK];

		// フィッシャールール時における時間
		else if (token == "winc")      is >> limits.inc[WHITE];
		else if (token == "binc")      is >> limits.inc[BLACK];

		// "go rtime 100"だと100～300[ms]思考する。
		else if (token == "rtime")     is >> limits.rtime;

		// 秒読み設定。
		else if (token == "byoyomi") {
			TimePoint t = 0;
			is >> t;

			// USIプロトコルでは、これが先手後手同じ値だと解釈する。
			limits.byoyomi[BLACK] = limits.byoyomi[WHITE] = t;
		}
	}

	for (const auto& thread : threads) {
		thread->worker->limits = limits;
	}

	return token_processed;
}

// "position"コマンド処理部
void USI::position(Position& pos, istringstream& is, StateListPtr& states)
{
	Move m;
	string token, sfen;

	is >> token;

	if (token == "startpos")
	{
		// 初期局面として初期局面のFEN形式の入力が与えられたとみなして処理する。
		sfen = SFEN_HIRATE;
		is >> token; // もしあるなら"moves"トークンを消費する。
	}
	// 局面がfen形式で指定されているなら、その局面を読み込む。
	// UCI(チェスプロトコル)ではなくUSI(将棋用プロトコル)だとここの文字列は"fen"ではなく"sfen"
	// この"sfen"という文字列は省略可能にしたいので..
	else {
		if (token != "sfen")
			sfen += token + " ";
		while (is >> token && token != "moves")
			sfen += token + " ";
	}

	// 新しく渡す局面なので古いものは捨てて新しいものを作る。
	states = StateListPtr(new StateList(1));
	pos.set(sfen, &states->back());

	std::vector<Move> moves_from_game_root;

	// 指し手のリストをパースする(あるなら)
	while (is >> token && (m = USI::to_move(pos, token)) != Move::none())
	{
		// 1手進めるごとにStateInfoが積まれていく。これは千日手の検出のために必要。
		states->emplace_back();
		if (m == Move::null()) // do_move に MOVE_NULL を与えると死ぬので
			pos.do_null_move(states->back(), tt);
		else
			pos.do_move(m, states->back(), tt);

		moves_from_game_root.emplace_back(m);
	}

	// やねうら王では、ここに保存しておくことになっている。
	threads.main_manager()->game_root_sfen = sfen;
	threads.main_manager()->moves_from_game_root = std::move(moves_from_game_root);

	// 盤面を設定しなおしたのでこのフラグはfalseに。
	threads.main_manager()->position_is_dirty = false;
}

// "setoption"コマンド応答。
void USI::setoption(istringstream& is)
{
	string token, name, value;

	while (is >> token && token != "value")
		// "name"トークンはあってもなくても良いものとする。(手打ちでコマンドを打つときには省略したい)
		if (token != "name")
			// スペース区切りで長い名前のoptionを使うことがあるので2つ目以降はスペースを入れてやる
			name += (name.empty() ? "" : " ") + token;

	// valueの後ろ。スペース区切りで複数文字列が来ることがある。
	while (is >> token)
		value += (value.empty() ? "" : " ") + token;

	if (options.count(name))
		options[name] = value;
	else
		// この名前のoptionは存在しなかった
		sync_cout << "info string Error! : No such option: " << name << sync_endl;

}

// --- 以下、やねうら王、独自拡張。

// "gameover"コマンドに対するハンドラ
#if defined(USE_GAMEOVER_HANDLER) || defined(YANEURAOU_ENGINE_DEEP)
void USI::gameover(const string& cmd);
#endif

// is_ready_cmd()を外部から呼び出せるようにしておく。(benchコマンドなどから呼び出したいため)
// 局面は初期化されないので注意。
void USI::isready(bool skipCorruptCheck)
{
	// EvalDirにある"eval_options.txt"を読み込む。
	// ここに評価関数に応じた設定を書いておくことができる。

	USI::read_engine_options(Path::Combine(options["EvalDir"], "eval_options.txt"));

	// yaneuraou.wasm
	// ブラウザのメインスレッドをブロックしないよう、Keep Alive処理をコメントアウト
#if !defined(__EMSCRIPTEN__)
	// --- Keep Alive的な処理 ---

	// "isready"を受け取ったあと、"readyok"を返すまで5秒ごとに改行を送るように修正する。(keep alive的な処理)
	// →　これ、よくない仕様であった。
	// cf. USIプロトコルでisready後の初期化に時間がかかる時にどうすれば良いのか？
	//     http://yaneuraou.yaneu.com/2020/01/05/usi%e3%83%97%e3%83%ad%e3%83%88%e3%82%b3%e3%83%ab%e3%81%a7isready%e5%be%8c%e3%81%ae%e5%88%9d%e6%9c%9f%e5%8c%96%e3%81%ab%e6%99%82%e9%96%93%e3%81%8c%e3%81%8b%e3%81%8b%e3%82%8b%e6%99%82%e3%81%ab%e3%81%a9/
	// cf. isready後のkeep alive用改行コードの送信について
	//		http://yaneuraou.yaneu.com/2020/03/08/isready%e5%be%8c%e3%81%aekeep-alive%e7%94%a8%e6%94%b9%e8%a1%8c%e3%82%b3%e3%83%bc%e3%83%89%e3%81%ae%e9%80%81%e4%bf%a1%e3%81%ab%e3%81%a4%e3%81%84%e3%81%a6/

	// これを送らないと、将棋所、ShogiGUIでタイムアウトになりかねない。
	// ワーカースレッドを一つ生成して、そいつが5秒おきに改行を送信するようにする。
	// このあと重い処理を行うのでスレッドの起動が遅延する可能性があるから、先にスレッドを生成して、そのスレッドが起動したことを
	// 確認してから処理を行う。

	// スレッドが起動したことを通知するためのフラグ
	auto thread_started = false;

	// この関数を抜ける時に立つフラグ(スレッドを停止させる用)
	auto thread_end = false;

	// 定期的な改行送信用のスレッド
	auto th = std::thread([&] {
		// スレッドが起動した
		thread_started = true;

		int count = 0;
		while (!thread_end)
		{
			std::this_thread::sleep_for(std::chrono::milliseconds(100));
			if (++count >= 50 /* 5秒 */)
			{
				count = 0;
				sync_cout << sync_endl; // 改行を送信する。

				// 定跡の読み込み部などで"info string.."で途中経過を出力する場合、
				// sync_cout ～ sync_endlを用いて送信しないと、この改行を送るタイミングとかち合うと
				// 変なところで改行されてしまうので注意。
			}
		}
		});
	SCOPE_EXIT({ thread_end = true; th.join(); });

	// スレッド起動待ち
	while (!thread_started)
		Tools::sleep(100);

	// --- Keep Alive的な処理ここまで ---
#endif

	// スレッドを先に生成しないとUSI_Hashで確保したメモリクリアの並列化が行われなくて困る。

	threads.set({ options, threads, tt });

#if defined (USE_EVAL_HASH)
	Eval::EvalHash_Resize(Options["EvalHash"]);
#endif

	// 評価関数の読み込み

#if defined(YANEURAOU_ENGINE_DEEP)

	// 毎回、load_eval()は呼び出すものとする。
	// モデルファイル名に変更がなければ、再読み込みされないような作りになっているならばこの実装のほうがシンプル。
	Eval::load_eval();
	USI::load_eval_finished = true;

#else

	// 評価関数の読み込みなど時間のかかるであろう処理はこのタイミングで行なう。
	// 起動時に時間のかかる処理をしてしまうと将棋所がタイムアウト判定をして、思考エンジンとしての認識をリタイアしてしまう。
	if (!USI::load_eval_finished)
	{
		// 評価関数の読み込み
		Eval::load_eval();

		// チェックサムの計算と保存(その後のメモリ破損のチェックのため)
		eval_sum = Eval::calc_check_sum();

		// ソフト名の表示
		Eval::print_softname(eval_sum);

		USI::load_eval_finished = true;
	}
	else
	{
		// メモリが破壊されていないかを調べるためにチェックサムを毎回調べる。
		// 時間が少しもったいない気もするが.. 0.1秒ぐらいのことなので良しとする。
		if (!skipCorruptCheck && eval_sum != Eval::calc_check_sum())
			sync_cout << "info string Error! : EVAL memory is corrupted" << sync_endl;
	}
#endif

	// isreadyに対してはreadyokを返すまで次のコマンドが来ないことは約束されているので
	// このタイミングで各種変数の初期化もしておく。

#if defined(YANEURAOU_ENGINE_DEEP)
	// ふかうら王では置換表は用いない。
#else
	tt.resize(size_t(options["USI_Hash"]));
#endif

	Search::clear(options, threads, tt);

#if defined (USE_EVAL_HASH)
	Eval::EvalHash_Clear();
#endif

	threads.stop = false;
}

// isreadyコマンド処理部
void USI::isready(Position& pos, StateListPtr& states)
{
	// 対局ごとに"isready","usinewgame"の両方が来る。
	// "isready"が起動後に1度だけしか来ないようなGUI実装は、
	// 実装上の誤りであるから修正すべきである。)

	// 少なくとも将棋のGUI(将棋所、ShogiGUI、将棋神やねうら王)では、
	// "isready"が毎回来るようなので、"usinewgame"のほうは無視して、
	// "isready"に応じて評価関数、定跡、探索部を初期化する。

	isready();

	// Positionコマンドが送られてくるまで評価値の全計算をしていないの気持ち悪いのでisreadyコマンドに対して
	// evalの値を返せるようにこのタイミングで平手局面で初期化してしまう。

	// 新しく渡す局面なので古いものは捨てて新しいものを作る。
	states = StateListPtr(new StateList(1));
	pos.set_hirate(&states->back());

	sync_cout << "readyok" << sync_endl;
}

// 評価関数を読み込んだかのフラグ。これはevaldirの変更にともなってfalseにする。
bool USI::load_eval_finished = false;

// 入玉ルール
#if defined(USE_ENTERING_KING_WIN)

	// 文字列に対応するEnteringKingRuleを取得する。
EnteringKingRule USI::to_entering_king_rule(const std::string& rule)
{
	for (size_t i = 0; i < USI::ekr_rules.size(); ++i)
		if (USI::ekr_rules[i] == rule)
			return (EnteringKingRule)i;

	ASSERT(false);
	return EnteringKingRule::EKR_NONE;
}
#endif

// 思考エンジンがGUIからの"usi"に対して返す"option ..."文字列から
// Optionオブジェクトを構築して、それをOptions[]に突っ込む。
// "engine_options.txt"というファイルの各行からOptionオブジェクト構築して
// Options[]の値を上書きするためにこの関数が必要。
// "option name USI_Hash type spin default 256"
// のような文字列が引数として渡される。
// このとき、Optionのhandlerとidxは書き換えない。
void USI::build_option(const std::string& line)
{
	// 1. "option ..."の形式

	// 2. エンジンオプション名と値だけを指定する形式で書かれているのかも知れない。
	// (既存のオプションの値のoverrideがしたい場合)

	// 3. オプション名=値 のような形式かも知れない(dlshogiの.iniはそうなっている)
	// その形式にも対応する必要がある。
	// よって、最初に"="を" "に置換してしまう。そうすれば、3.は、2.の形式と同じになる。

	const auto& line2 = StringExtension::Replace(line, '=', ' ');

	Parser::LineScanner scanner(line2);
	string token0 = scanner.get_text();
	if (token0 != "option")
	{
		// 空行は無視
		if (token0 == "")
			return;

		auto it = options.find(token0);
		if (it == options.end())
		{
			// 違うのか。何の形式で書こうとしているのだろうか…。
			std::cout << "Error : option name not found : " << token0 << std::endl;
			return;
		}

		// オプション設定の強制的な置換
		// min = max = default = この値　になる。
		auto param = scanner.get_text();
		options[token0].overwrite(param);

		sync_cout << "info string engine option override. name = " << token0 << " , value = " << param << sync_endl;
	}
	else {

		string name, value, option_type;
		int64_t min_value = 0, max_value = 1;
		std::vector<string> combo_list;
		while (!scanner.eol())
		{
			string token = scanner.get_text();
			if (token == "name") name = scanner.get_text();
			else if (token == "type") option_type = scanner.get_text();
			else if (token == "default") value = scanner.get_text();
			else if (token == "min") min_value = stoll(scanner.get_text());
			else if (token == "max") max_value = stoll(scanner.get_text());
			else if (token == "var") {
				auto varText = scanner.get_text();
				combo_list.push_back(varText);
			}
			else {
				std::cout << "Error : invalid command: " << token << std::endl;
			}
		}

		if (options.count(name) != 0)
		{
			// typeに応じたOptionの型を生成して代入する。このときに "<<"を用いるとidxが変わってしまうので overwriteで代入する。
			if (option_type == "check") options[name].overwrite(Option(value == "true"));
			else if (option_type == "spin") options[name].overwrite(Option(stoll(value), min_value, max_value));
			else if (option_type == "string") options[name].overwrite(Option(value.c_str()));
			else if (option_type == "combo") options[name].overwrite(Option(combo_list, value));
		}
		else
			std::cout << "Error : option name not found : " << name << std::endl;
	}

}

// エンジンオプションをコンパイル時に設定する機能
// "ENGINE_OPTIONS"で指定した内容を設定する。
// 例) #define ENGINE_OPTIONS "FV_SCALE=24;BookFile=no_book"
void USI::set_engine_options(const string& options_string)
{
	// ";"で区切って複数指定できるものとする。
	auto v = StringExtension::Split(options_string, ";");
	for (auto line : v)
		build_option(std::string(line));
}

// カレントフォルダに"engine_options.txt"(これは引数で指定されている)が
// あればそれをオプションとしてOptions[]の値をオーバーライドする機能。
void USI::read_engine_options(const string& filename)
{
	SystemIO::TextReader reader;
	if (reader.Open(filename).is_not_ok())
		return;

	string line;
	while (reader.ReadLine(line).is_ok())
		build_option(line);
}

// getoptionコマンド応答(USI独自拡張)
// オプションの値を取得する。
void USI::getoption(istringstream& is)
{
	// getoption オプション名
	string name = "";
	is >> name;

	// すべてを出力するモード
	bool all = name == "";

	for (auto& o : options)
	{
		// 大文字、小文字を無視して比較。また、nameが指定されていなければすべてのオプション設定の現在の値を表示。
		if ((!StringExtension::stricmp(name, o.first)) || all)
		{
			sync_cout << "Options[" << o.first << "] == " << (string)options[o.first] << sync_endl;
			if (!all)
				return;
		}
	}
	if (!all)
		sync_cout << "No such option: " << name << sync_endl;
}

// --------------------
// テスト用にqsearch(),search()を直接呼ぶ
// --------------------

#if defined(EVAL_LEARN)
void USI::qsearch(Position& pos)
{
	cout << "qsearch : ";
	auto pv = Learner::qsearch(pos, options, *threads.main_thread()->worker);
	cout << "Value = " << pv.first << " , PV = ";
	for (auto m : pv.second)
		cout << m << " ";
	cout << endl;
}

void USI::search(Position& pos, istringstream& is)
{
	string token;
	int depth = 1;
	int multi_pv = (int)options["MultiPV"];
	while (is >> token)
	{
		if (token == "depth")
			is >> depth;
		if (token == "multipv")
			is >> multi_pv;
	}

	cout << "search depth = " << depth << " , multi_pv = " << multi_pv << " : ";
	auto pv = Learner::search(pos, depth, multi_pv, 0, options, *threads.main_thread()->worker, threads);
	cout << "Value = " << pv.first << " , PV = ";
	for (auto m : pv.second)
		cout << m << " ";
	cout << endl;
}

#endif

// ----------------------------------
//      USI拡張コマンド "test"
// ----------------------------------

#if defined(ENABLE_TEST_CMD)

// USI拡張コマンドのうち、開発上のテスト関係のコマンド。
// 思考エンジンの実行には関係しない。

namespace Test
{
	// 通常のテスト用コマンド。コマンドを処理した時 trueが返る。
	bool normal_test_cmd(Position& pos, std::istringstream& is, const std::string& token);

	// 詰み関係のテスト用コマンド。コマンドを処理した時 trueが返る。
	bool mate_test_cmd(Position& pos, std::istringstream& is, const std::string& token);
}

void USI::test(Position& pos, std::istringstream& is)
{
	// 探索をするかも知れないので初期化しておく。
	isready();

	std::string token;
	is >> token;

	// デザパタのDecoratorの呼び出しみたいな感じで書いていく。

	// 通常のテスト用コマンド
	if (Test::normal_test_cmd(pos, is, token))
		return;

	// 詰み関係のテスト用コマンド
	if (Test::mate_test_cmd(pos, is, token))
		return;

	sync_cout << "info string Error! : unknown command = " << token << sync_endl;
}

#endif // defined(ENABLE_TEST_CMD)

//
// あとで整理する
//


// ユーザーの実験用に開放している関数。
// USI拡張コマンドで"user"と入力するとこの関数が呼び出される。
// "user"コマンドの後続に指定されている文字列はisのほうに渡される。
void user_test(Position& pos, std::istringstream& is);

#if defined(ENABLE_TEST_CMD)
	void generate_moves_cmd(Position& pos);
#endif

#if defined(USE_MATE_DFPN)
// "mate"コマンド
void mate_cmd(Position& pos, istream& is);
#endif





// ----------------------------------
//   USI拡張コマンド "cluster"
//     やねうら王 The Cluster
// ----------------------------------

#if defined(USE_YO_CLUSTER)
#if defined(YANEURAOU_ENGINE_DEEP) || defined(YANEURAOU_ENGINE_NNUE)
namespace YaneuraouTheCluster
{
	// cluster時のUSIメッセージの処理ループ
	void cluster_usi_loop(Position& pos, std::istringstream& is);
}
#endif
#endif

// --------------------
// USI関係のコマンド処理
// --------------------

// check sumを計算したとき、それを保存しておいてあとで次回以降、整合性のチェックを行なう。
u64 USI::eval_sum;









// --------------------
// USI関係の記法変換部
// --------------------

namespace {
	// USIの指し手文字列などに使われている盤上の升を表す文字列をSquare型に変換する
	// 変換できなかった場合はSQ_NBが返る。高速化のために用意した。
	Square usi_to_sq(char f, char r)
	{
		File file = toFile(f);
		Rank rank = toRank(r);

		if (is_ok(file) && is_ok(rank))
			return file | rank;

		return SQ_NB;
	}
}

#if defined(USE_PIECE_VALUE)
/// Turns a Value to an integer centipawn number,
/// without treatment of mate and similar special scores.
// 詰みやそれに類似した特別なスコアの処理なしに、Valueを整数のセントポーン数に変換します、
int USI::to_cp(Value v) {

  return 100 * v / NormalizeToPawnValue;
}

// cpからValueへ。⇑の逆変換。
Value USI::cp_to_value(int v)
{
	return Value((std::abs(v) < VALUE_MATE_IN_MAX_PLY) ? (NormalizeToPawnValue * v / 100) : v);
}

// スコアを歩の価値を100として正規化して出力する。
// USE_PIECE_VALUEが定義されていない時は正規化しようがないのでこの関数は呼び出せない。
std::string USI::value(Value v)
{
	ASSERT_LV3(-VALUE_INFINITE < v && v < VALUE_INFINITE);

	std::stringstream ss;

	// 置換表上、値が確定していないことがある。
	if (v == VALUE_NONE)
		ss << "none";
	else if (std::abs(v) < VALUE_MATE_IN_MAX_PLY)
		//s << "cp " << v * 100 / int(Eval::PawnValue);
		ss << "cp " << USI::to_cp(v);
	/*
    else if (abs(v) <= VALUE_TB)
    {
        const int ply = VALUE_TB - std::abs(v);  // recompute ss->ply
        ss << "cp " << (v > 0 ? 20000 - ply : -20000 + ply);
    }
	*/
	else if (v == -VALUE_MATE)
		// USIプロトコルでは、手数がわからないときには "mate -"と出力するらしい。
		// 手数がわからないというか詰んでいるのだが…。これを出力する方法がUSIプロトコルで定められていない。
		// ここでは"-0"を出力しておく。
		// ※　ShogiGUIだと、これで"+詰"と出力されるようである。
		ss << "mate -0";
	else
		ss << "mate " << (v > 0 ? VALUE_MATE - v : -VALUE_MATE - v);

	return ss.str();
}

#endif

// Square型をUSI文字列に変換する
std::string USI::square(Square s) {
	return std::string{ char('a' + file_of(s)), char('1' + rank_of(s)) };
}

// 指し手をUSI文字列に変換する。
std::string USI::move(Move   m) { return move(m.to_move16()); }
std::string USI::move(Move16 m)
{
	std::stringstream ss;
	if (!m.is_ok())
	{
		ss << ((m.to_u16() == MOVE_RESIGN) ? "resign" :
			   (m.to_u16() == MOVE_WIN)    ? "win" :
			   (m.to_u16() == MOVE_NULL)   ? "null" :
			   (m.to_u16() == MOVE_NONE)   ? "none" :
			    "");
	}
	else if (m.is_drop())
	{
		ss << m.move_dropped_piece();
		ss << '*';
		ss << m.to_sq();
	}
	else {
		ss << m.from_sq();
		ss << m.to_sq();
		if (m.is_promote())
			ss << '+';
	}
	return ss.str();
}

// 読み筋をUSI文字列化して返す。
// " 7g7f 8c8d" のように返る。
std::string USI::move(const std::vector<Move>& moves)
{
	std::ostringstream oss;
	for (const auto& move : moves) {
		oss << " " << move;
	}
	return oss.str();
}


// 局面posとUSIプロトコルによる指し手を与えてもし可能なら等価で合法な指し手を返す。
// また合法でない指し手の場合、エラーである旨を出力する。
Move USI::to_move(const Position& pos, const std::string& str)
{
	// 全合法手のなかからusi文字列に変換したときにstrと一致する指し手を探してそれを返す
	//for (const ExtMove& ms : MoveList<LEGAL_ALL>(pos))
	//  if (str == move_to_usi(ms.move))
	//    return ms.move;

	// ↑のコードは大変美しいコードではあるが、棋譜を大量に読み込むときに時間がかかるうるのでもっと高速な実装をする。

	if (str == "resign")
		return Move::resign();

	if (str == "win")
		return Move::win();

	// パス(null move)入力への対応 {UCI: "0000", GPSfish: "pass"}
	if (str == "0000" || str == "null" || str == "pass")
		return Move::null();

	// usi文字列を高速にmoveに変換するやつがいるがな..
	Move move = pos.to_move(USI::to_move16(str));

	// 現在の局面に至る手順として歩の不成が与えられることはあるので、
	// pseudo_legal_s<true>()で判定する。
	if (pos.pseudo_legal_s<true>(move) && pos.legal(move))
		return move;

	// 入力に非合法手が含まれていた。エラーとして出力すべき。
	sync_cout << "info string Error! : Illegal Input Move : " << str << sync_endl;

	return Move::none();
}


// USI形式から指し手への変換。本来この関数は要らないのだが、
// 棋譜を大量に読み込む都合、この部分をそこそこ高速化しておきたい。
// やねうら王、独自追加。
Move16 USI::to_move16(const string& str)
{
	Move16 move = Move16::none();

	{
		// さすがに3文字以下の指し手はおかしいだろ。
		if (str.length() <= 3)
			goto END;

		Square to = usi_to_sq(str[2], str[3]);
		if (!is_ok(to))
			goto END;

		bool promote = str.length() == 5 && str[4] == '+';
		bool drop = str[1] == '*';

		if (!drop)
		{
			Square from = usi_to_sq(str[0], str[1]);
			if (is_ok(from))
				move = promote ? make_move_promote16(from, to) : make_move16(from, to);
		}
		else
		{
			for (int i = 1; i <= 7; ++i)
				if (PieceToCharBW[i] == str[0])
				{
					move = make_move_drop16((PieceType)i, to);
					break;
				}
		}
	}

END:
	return move;
}

// --------------------
//    読み筋の出力
// --------------------

// depth : iteration深さ
std::string USI::pv(
	const Search::Worker& workerThread, Position& pos, TimePoint elapsed,
	uint64_t nodesSearched, int hashfull, Search::LimitsType& limits,
	TranspositionTable& tt)
{
	std::stringstream ss;

#if defined(__EMSCRIPTEN__)
	// yaneuraou.wasm
	// Time.elapsed()が-1を返すことがある
	// https://github.com/lichess-org/stockfish.wasm/issues/5
	// https://github.com/lichess-org/stockfish.wasm/commit/4f591186650ab9729705dc01dec1b2d099cd5e29
	elapsed = std::max(elapsed, TimePoint(1));
#endif
	const auto& rootMoves = workerThread.rootMoves;
	const auto& depth = workerThread.completedDepth;
	size_t pvIdx = workerThread.pvIdx;
	size_t multiPV = std::min(size_t(workerThread.options["MultiPV"]), rootMoves.size());

	uint64_t nodes_searched = nodesSearched;

	// MultiPVでは上位N個の候補手と読み筋を出力する必要がある。
	for (size_t i = 0; i < multiPV; ++i)
	{
		// この指し手のpvの更新が終わっているのか
		bool updated = rootMoves[i].score != -VALUE_INFINITE;

		if (depth == 1 && !updated && i > 0)
			continue;

		// 1より小さな探索depthで出力しない。
		Depth d = updated ? depth : std::max(1, depth - 1);
		Value v = updated ? rootMoves[i].usiScore : rootMoves[i].previousScore;

		// multi pv時、例えば3個目の候補手までしか評価が終わっていなくて(PVIdx==2)、このとき、
		// 3,4,5個目にあるのは前回のiterationまでずっと評価されていなかった指し手であるような場合に、
		// これらのpreviousScoreが-VALUE_INFINITE(未初期化状態)でありうる。
		// (multi pv状態で"go infinite"～"stop"を繰り返すとこの現象が発生する。おそらく置換表にhitしまくる結果ではないかと思う。)
		if (v == -VALUE_INFINITE)
			v = VALUE_ZERO; // この場合でもとりあえず出力は行う。

		//bool tb = TB::RootInTB && abs(v) < VALUE_MATE_IN_MAX_PLY;
		//v = tb ? rootMoves[i].tbScore : v;

		if (ss.rdbuf()->in_avail()) // 1行目でないなら連結のための改行を出力
			ss << std::endl;

		ss << "info"
			<< " depth " << d
			<< " seldepth " << rootMoves[i].selDepth
#if defined(USE_PIECE_VALUE)
			<< " score " << USI::value(v)
#endif
			;

		// これが現在探索中の指し手であるなら、それがlowerboundかupperboundかは表示させる
		if (i == pvIdx && /*!tb &&*/ updated) // tablebase- and previous-scores are exact
			ss << (rootMoves[i].scoreLowerbound ? " lowerbound" : (rootMoves[i].scoreUpperbound ? " upperbound" : ""));

		// 将棋所はmultipvに対応していないが、とりあえず出力はしておく。
		if (multiPV > 1)
			ss << " multipv " << (i + 1);

		ss << " nodes " << nodes_searched
			<< " nps " << nodes_searched * 1000 / elapsed
			<< " hashfull " << hashfull
			<< " time " << elapsed
			<< " pv";


		// PV配列からPVを出力する。
		// ※　USIの"info"で読み筋を出力するときは"pv"サブコマンドはサブコマンドの一番最後にしなければならない。

		auto out_array_pv = [&]()
			{
				for (Move m : rootMoves[i].pv)
					ss << " " << m;
			};

		// 置換表からPVをかき集めてきてPVを出力する。
		auto out_tt_pv = [&]()
			{
				auto pos_ = const_cast<Position*>(&pos);
				Move moves[MAX_PLY + 1];
				StateInfo si[MAX_PLY];
				int ply = 0;

				while (ply < MAX_PLY)
				{
					// 千日手はそこで終了。ただし初手はPVを出力。
					// 千日手がベストのとき、置換表を更新していないので
					// 置換表上はMove::none()がベストの指し手になっている可能性があるので早めに検出する。
					auto rep = pos.is_repetition(ply);
					if (rep != REPETITION_NONE && ply >= 1)
					{
						// 千日手でPVを打ち切るときはその旨を表示
						ss << " " << rep;
						break;
					}

					Move m;

					// まず、rootMoves.pvを辿れるところまで辿る。
					// rootMoves[i].pv[0]は宣言勝ちの指し手(Move::win())の可能性があるので注意。
					if (ply < int(rootMoves[i].pv.size()))
						m = rootMoves[i].pv[ply];
					else
					{
						// 次の手を置換表から拾う。
						// ただし置換表を破壊されるとbenchコマンドの時にシングルスレッドなのに探索内容の同一性が保証されなくて
						// 困るのでread_probe()を用いる。

						auto [ttHit, ttData, ttWriter] = tt.probe(pos.key(), pos);
						// 置換表になかった
						if (!ttHit)
							break;

						m = ttData.move;

						// leaf nodeはわりと高い確率でMove::none()
						if (m == Move::none())
							break;

						// 置換表にはpsudo_legalではない指し手が含まれるのでそれを弾く。
						// 宣言勝ちでないならこれが合法手であるかのチェックが必要。
						if (m != Move::win())
						{
							// 歩の不成が読み筋に含まれていようともそれは表示できなくてはならないので
							// pseudo_legal_s<true>()を用いて判定。
							if (!(pos.pseudo_legal_s<true>(m) && pos.legal(m)))
								break;
						}
					}

#if defined (USE_ENTERING_KING_WIN)
					// 宣言勝ちである
					if (m == Move::win())
					{
						// これが合法手であるなら宣言勝ちであると出力。
						if (pos.DeclarationWin(limits) != Move::none())
							ss << " " << Move::win();

						break;
					}
#endif
					// leaf node末尾にMove::resign()があることはないが、
					// 詰み局面で呼び出されると1手先がmove resignなので、これでdo_move()するのは
					// 非合法だから、do_move()せずにループを抜ける。
					if (!m.is_ok())
					{
						ss << " " << m;
						break;
					}

					moves[ply] = m;
					ss << " " << m;

					// 注)
					// このdo_moveで Position::nodesが加算されるので探索ノード数に影響が出る。
					// benchコマンドで探索ノード数が一致しない場合、これが原因。
					// → benchコマンドでは、ConsiderationMode = falseにすることで
					// 　PV表示のためにdo_move()を呼び出さないようにした。

					pos_->do_move(m, si[ply], tt);
					++ply;
				}
				while (ply > 0)
					pos_->undo_move(moves[--ply]);
			};

		// 検討用のPVを出力するモードなら、置換表からPVをかき集める。
		// (そうしないとMultiPV時にPVが欠損することがあるようだ)
		// fail-highのときにもPVを更新しているのが問題ではなさそう。
		// Stockfish側の何らかのバグかも。
		if (limits.consideration_mode)
			out_tt_pv();
		else
			out_array_pv();
	}

	return ss.str();
}

// namespace USI内のUnitTest。
void USI::UnitTest(Test::UnitTester& tester)
{
	auto section1 = tester.section("USI");

	Position pos;
	StateInfo si;

	// 平手初期化
	auto hirate_init = [&] { pos.set_hirate(&si); };

	// SFEN文字列でのPosition初期化
	auto sfen_init = [&](const string& sfen) { pos.set(sfen, &si); };

	// Search::Limitsのalias
	auto& limits = threads.main_thread()->worker->limits;

	{

		auto section2 = tester.section("to_move()");
		{
			//auto section3 = tester.section("unpromoted pawn move");

			sfen_init("2sgkgs2/9/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL w - 1");

			// いま不成を生成するオプションがオフであると仮定する。
			limits.generate_all_legal_moves = false;

			auto moves = "6a5b 1g1f 4a3b 1f1e 3a2b 2g2f 2c2d 7g7f 3b2c 2f2e 2d2e 2h2e P*2d 2e9e 7a8b 8h6f 8c8d 3i3h 5b6b 7i6h 6b7b 9e9f 7b8c 6h7g 7c7d 7f7e 7d7e 7g8f 8b7c 1e1d 1c1d 8f7e P*7d 7e8f 5a5b P*7b 5b6b 7b7a+ 6b7a P*7e 7a6b 7e7d 7c7d 8f9e P*7e 9f8f 8d8e 8f9f 5c5d 5g5f 6c6d 5f5e 6b6c 5e5d 6c5d 8i7g 6d6e 6f5g 2c3d 6i7h 3d4e 4g4f P*5f 5g6h 4e5e 3h4g 6e6f 4f4e 9c9d 9e9d 8c8d 6g6f P*9e 9d9c 8d9d P*5g 9e9f P*2c 2b2c 5g5f 5e4e P*4f 4e4d 4f4e 4d4e 9c8b 7e7f 7g6e 7d7e P*4f 4e4d 4f4e 4d4e 7h6g R*8i 5i4h 8i9i+ P*4f 4e4d 4f4e 4d4e 6e7c+ 4c4d 7c7d 9d8d 7d8d 7e8d 4g4f 4e4f 6h4f L*4e 6g5g 4e4f 5g4f 5d4c 5f5e P*5c G*2b 2c3d L*5i 4d4e 4f3f 4e4f 3f4f P*4e 4f3f 4e4f 3f4f P*4e 4f3f 4e4f 3f4f P*4e 4f3f 4e4f 3f4f P*4e 4f3f 4e4f 3f4f P*4e 4f3f N*4d 4h3h 4d3f 3g3f 4e4f P*2g S*4g 3h2h 4g3f P*4d 4c4d P*4h 2d2e P*3g 3f2g 2h2g 2e2f 2g2f 3d3e 2f2e G*2d 2e1f 1d1e 1f1g P*2f N*3f 3e3f 3g3f B*2g S*3h 2g3f+ 1g2h N*3e S*1h 2f2g+ 1h2g 3e2g+ 3h2g 3f6c N*3f 4d3d 3f2d P*2f 2g2f S*2g 2h3i P*2h G*3g 2h2i+ 3i2i P*2h 2i3i N*5g 3g4f 5g4i+ 3i4i G*6h P*7i 9i7i G*5h 6h5h 4i5h 7i7h G*6h G*6g 5h4g 7h6h N*3f G*5g 4g3g 6h4h 3g2g 4h4f P*3g 3d4e S*3e 4f4g P*4i 4e5f 4i4h 4g4h 5i5g 5f5g 1i1e 5g5h 2g1f 5h5i P*4f 4h4i 1e1c+ 4i5h 1f1e 5h4g G*3i 4g5h 8b7a+ 6c6d 1e1d 6d5e 8g8f 8e8f P*8g 8f8g";
			// ↑この局面、最後の8f8gが歩の不成だが、これがUSI::to_move()で非合法手扱いされないかをテストする。
			// cf. https://github.com/yaneurao/YaneuraOu/issues/190

			istringstream is(moves);
			string token;
			bool fail = false;

			StateInfo si[512];
			while (is >> token)
			{
				Move m = USI::to_move(pos,token);
				if (m == Move::none())
					fail = true;

				pos.do_move(m, si[pos.game_ply()], tt);
			}

			tester.test("pawn's unpromoted move",!fail);

		}

	}
}

#if defined(__EMSCRIPTEN__)
// --------------------
// EMSCRIPTEN support
// --------------------
static StateListPtr states(new StateList(1));

// USI応答部 emscriptenインターフェース
EMSCRIPTEN_KEEPALIVE extern "C" int usi_command(const char *c_cmd) {
	std::string cmd(c_cmd);

	static Position pos;
	string token;

	for (Thread* th : Threads) {
		if (!th->threadStarted)
			return 1;
	}

	usi_cmdexec(pos, states, cmd);

	return 0;
}
#endif
