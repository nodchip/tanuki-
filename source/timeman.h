#ifndef TIMEMAN_H_INCLUDED
#define TIMEMAN_H_INCLUDED

#include "misc.h"
#include "usi_option.h"

// -----------------------
//  探索のときに使う時間管理用
// -----------------------

namespace Search { struct LimitsType; }

struct TimeManagement
{
	// タイマーを初期化する。以降、elapsed()でinit()してからの経過時間が得られる。
	void reset() { startTime = startTimeFromPonderhit = now(); }

	// "ponderhit"からの時刻を計測する用
	void reset_for_ponderhit() { startTimeFromPonderhit = now(); }

	// 探索開始からの経過時間。単位は[ms]
	// 探索node数に縛りがある場合、elapsed()で探索node数が返ってくる仕様にすることにより、一元管理できる。
	TimePoint elapsed() const;

	// reset_for_ponderhit()からの経過時間。その関数は"ponderhit"したときに呼び出される。
	// reset_for_ponderhit()が呼び出されていないときは、reset()からの経過時間。その関数は"go"コマンドでの探索開始時に呼び出される。
	TimePoint elapsed_from_ponderhit() const;

	// reset()されてからreset_for_ponderhit()までの時間
	TimePoint elapsed_from_start_to_ponderhit() const { return (TimePoint)(startTimeFromPonderhit - startTime); }

#if 0
	// 探索node数を経過時間の代わりに使う。(こうするとタイマーに左右されない思考が出来るので、思考に再現性を持たせることが出来る)
	// node数を指定して探索するとき、探索できる残りnode数。
	// ※　StockfishでここintになっているのはTimePointにするのが正しいと思う。[2020/01/20]
	TimePoint availableNodes;
	// →　NetworkDelayやMinimumThinkingTimeなどの影響を考慮するのが難しく、将棋の場合、
	// 　相性があまりよろしくないのでこの機能はやねうら王ではサポートしないことにする。
#endif

	// このシンボルが定義されていると、今回の思考時間を計算する機能が有効になる。
#if defined(USE_TIME_MANAGEMENT)

	// 今回の思考時間を計算して、optimum(),maximum()が値をきちんと返せるようにする。
	// ※　ここで渡しているlimitsは、今回の探索の終わりまでなくならないものとする。
	//    "ponderhit"でreinit()でこの変数を参照することがあるため。
	void init(const Search::LimitsType& limits, Color us, int ply, OptionsMap& options);

	// ponderhitの時に残り時間が付与されている時(USI拡張)、再度思考時間を調整するために↑のinit()相当のことを行う。
	void reinit(OptionsMap& options) { init_(*lastcall_Limits, lastcall_Us, lastcall_Ply, options); }

	TimePoint minimum() const { return minimumTime; }
	TimePoint optimum() const { return optimumTime; }
	TimePoint maximum() const { return maximumTime; }

	// 1秒単位で繰り上げてdelayを引く。
	// ただし、remain_timeよりは小さくなるように制限する。
	TimePoint round_up(TimePoint t) const;

	// 探索終了の時間(startTime + search_end >= now()になったら停止)
	std::atomic<TimePoint> search_end;

private:
	TimePoint minimumTime;
	TimePoint optimumTime;
	TimePoint maximumTime;

	// Options["NetworkDelay"]の値
	TimePoint network_delay;
	// Options["MinimalThinkingTime"]の値
	TimePoint minimum_thinking_time;

	// 今回の残り時間 - Options["NetworkDelay2"]
	TimePoint remain_time;

	// init()の内部実装用。
	void init_(const Search::LimitsType& limits, Color us, int ply, OptionsMap& options);

	// init()が最後に呼び出された時に各引数。これを保存しておき、reinit()の時にはこれを渡す。
	Search::LimitsType* lastcall_Limits; // どこかに確保しっぱなしにするだろうからポインタでいいや…
	Color lastcall_Us;
	int lastcall_Ply;

#endif

private:
	// 探索開始時刻。
	TimePoint startTime;

	// reset()かreset_for_ponderhit()が呼び出された時刻。
	TimePoint startTimeFromPonderhit;
};


#endif // #ifndef TIMEMAN_H_INCLUDED
