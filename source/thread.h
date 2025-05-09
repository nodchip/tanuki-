#ifndef THREAD_H_INCLUDED
#define THREAD_H_INCLUDED

#include <atomic>
#include <condition_variable>
//#include <cstddef>
//#include <cstdint>
#include <mutex>
#include <vector>

#include "movepick.h"
#include "numa.h"
#include "position.h"
#include "search.h"
#include "thread_win32_osx.h"
//#include "types.h"

#if defined(EVAL_LEARN)
// 学習用の実行ファイルでは、スレッドごとに置換表を持ちたい。
#include "tt.h"
#endif

// --------------------
// スレッドの属するNumaを管理する
// --------------------

// Sometimes we don't want to actually bind the threads, but the recipient still
// needs to think it runs on *some* NUMA node, such that it can access structures
// that rely on NUMA node knowledge. This class encapsulates this optional process
// such that the recipient does not need to know whether the binding happened or not.

// 時にはスレッドを実際にバインドしたくない場合もありますが、
// 受け手側は、それが 何らかの NUMAノード上で実行されていると認識する必要があります。
// これは、NUMAノードに関する情報を必要とする構造体にアクセスするためです。
// このクラスは、このバインドが行われたかどうかを受け手が知る必要がないように、
// このオプションのプロセスをカプセル化します。

class OptionalThreadToNumaNodeBinder {
public:
	OptionalThreadToNumaNodeBinder(NumaIndex n) :
		numaConfig(nullptr),
		numaId(n) {}

	OptionalThreadToNumaNodeBinder(const NumaConfig& cfg, NumaIndex n) :
		numaConfig(&cfg),
		numaId(n) {}

	NumaReplicatedAccessToken operator()() const {
		if (numaConfig != nullptr)
			return numaConfig->bind_current_thread_to_numa_node(numaId);
		else
			return NumaReplicatedAccessToken(numaId);
	}

private:
	const NumaConfig* numaConfig;
	NumaIndex         numaId;
};


// --------------------
// 探索時に用いるスレッド
// --------------------

// Abstraction of a thread. It contains a pointer to the worker and a native thread.
// After construction, the native thread is started with idle_loop()
// waiting for a signal to start searching.
// When the signal is received, the thread starts searching and when
// the search is finished, it goes back to idle_loop() waiting for a new signal.

// (探索用の)スレッドの抽象化です。これはワーカーへのポインタとネイティブスレッドを含みます。
// 構築後、ネイティブスレッドは idle_loop() で開始され、開始信号を待ちます。
// 信号を受け取るとスレッドは検索を開始し、検索が終了すると再び idle_loop() に戻り、新しい信号を待ちます。
// ⇨  探索時に用いる、それぞれのスレッド。これを探索用スレッド数だけ確保する。
//    ただしメインスレッドはこのclassを継承してMainThreadにして使う。
class Thread
{
public:

	// ThreadPoolで何番目のthreadであるかをコンストラクタで渡すこと。この値は、idx(スレッドID)となる。
	explicit Thread(Search::SharedState&, std::unique_ptr<Search::ISearchManager>, size_t n);
	virtual ~Thread();

	// スレッド起動後、この関数が呼び出される。
	void idle_loop();

	// ------------------------------
	//      同期待ちのwait等
	// ------------------------------

	// Thread::search()を開始させるときに呼び出す。
	void start_searching();

	// 探索が終わるのを待機する。(searchingフラグがfalseになるのを待つ)
	void wait_for_search_finished();

	// Threadの自身のスレッド番号を返す。0 origin。
	size_t id() const { return idx; }

	std::unique_ptr<Search::Worker> worker;

	// === やねうら王独自拡張 ===

	// 探索中であるかを返す。
	bool is_searching() const { return searching; }

private:

	// exitフラグやsearchingフラグの状態を変更するときのmutex
	std::mutex mutex;

	// idle_loop()で待機しているときに待つ対象
	std::condition_variable cv;

	size_t idx, nthreads;

	// exit      : このフラグが立ったら終了する。
	// searching : 探索中であるかを表すフラグ。プログラムを簡素化するため、事前にtrueにしてある。
	bool exit = false, searching = true;

	// stack領域を増やしたstd::thread
	NativeThread stdThread;
};


// 思考で用いるスレッドの集合体
// 継承はあまり使いたくないが、for(auto* th:Threads) ... のようにして回せて便利なのでこうしてある。
//
// このクラスにコンストラクタとデストラクタは存在しない。
// Threads(スレッドオブジェクト)はglobalに配置するし、スレッドの初期化の際には
// スレッドが保持する思考エンジンが使う変数等がすべてが初期化されていて欲しいからである。
// スレッドの生成はset(options["Threads"])で行い、スレッドの終了はset(0)で行なう。
class ThreadPool
{
public:

	~ThreadPool() {
		// destroy any existing thread(s)
		if (threads.size() > 0)
		{
			main_thread()->wait_for_search_finished();

			while (threads.size() > 0)
				delete threads.back(), threads.pop_back();
		}
	}

	// mainスレッドに思考を開始させる。
	void start_thinking(const OptionsMap&, const Position& pos, StateListPtr& states , const Search::LimitsType& limits , bool ponderMode = false);

	// set()で生成したスレッドの初期化
	void clear();

	// スレッド数を変更する。
	void set(Search::SharedState);

	Search::SearchManager* main_manager() const {
		return static_cast<Search::SearchManager*>(main_thread()->worker.get()->manager.get());
	};

	// mainスレッドを取得する。これはthis[0]がそう。
	Thread* main_thread() const { return threads.front(); }

	// 今回、goコマンド以降に探索したノード数
	// →　これはPosition::do_move()を呼び出した回数。
	// ※　dlshogiエンジンで、探索ノード数が知りたい場合は、
	// 　dlshogi::nodes_visited()を呼び出すこと。
	uint64_t nodes_searched() const { return accumulate(&Search::Worker::nodes); }

	// 探索終了時に、一番良い探索ができていたスレッドを選ぶ。
	Thread* get_best_thread() const;

	// 探索を開始する(main thread以外)
	void start_searching();

	// main threadがそれ以外の探索threadの終了を待つ。
	void wait_for_search_finished() const;

	// stop          : 探索中にこれがtrueになったら探索を即座に終了すること。
	// increaseDepth : 一定間隔ごとに反復深化の探索depthが増えて行っているかをチェックするためのフラグ
	//                 増えて行ってないなら、同じ深さを再度探索するのに用いる。
	std::atomic_bool stop , increaseDepth;

	auto cbegin() const noexcept { return threads.cbegin(); }
	auto begin() const noexcept { return threads.begin(); }
	auto begin() noexcept { return threads.begin(); }
	auto end() noexcept { return threads.end(); }
	auto end() const noexcept { return threads.end(); }
	auto cend() const noexcept { return threads.cend(); }
	auto size() const noexcept { return threads.size(); }
	auto empty() const noexcept { return threads.empty(); }
	// thread_pool[n]のようにでアクセスしたいので…。
	auto operator[](size_t i) const noexcept { return threads[i];}

	// === やねうら王独自拡張 ===

	// main thread以外の探索スレッドがすべて終了しているか。
	// すべて終了していればtrueが返る。
	bool search_finished() const;

private:

	// 現局面までのStateInfoのlist
	StateListPtr setupStates;

	// vector<Thread*>からこのclassを継承させるのはやめて、このメンバーとして持たせるようにした。
	std::vector<Thread*> threads;

	// Threadクラスの特定のメンバー変数を足し合わせたものを返す。
	uint64_t accumulate(std::atomic<uint64_t> Search::Worker::* member) const {

		uint64_t sum = 0;
		for (Thread* th : threads)
			sum += (th->worker.get()->*member).load(std::memory_order_relaxed);
		return sum;
	}
};

#endif // #ifndef THREAD_H_INCLUDED

