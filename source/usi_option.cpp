
//#include "evaluate.h"
#include "misc.h"
//#include "search.h
//#include "syzygy/tbprobe.h"
#include "thread.h"
//#include "tt.h"
#include "usi.h"

using std::string;

/// 'On change' actions, triggered by an option's value change
// オプションの値が変更された時に呼び出されるOn changeアクション。

//static void on_clear_hash(const Option&) { Search::clear(); }
//static void on_hash_size(const Option& o) { TT.resize(size_t(o)); }
static void on_logger(const Option& o) { start_logger(o); }
//static void on_threads(const Option& o) { Threads.set(size_t(o)); }
//static void on_tb_path(const Option& o) { Tablebases::init(o); }
//static void on_eval_file(const Option&) { Eval::NNUE::init(); }

// --- やねうら王独自拡張分の前方宣言

// USIプロトコルで必要とされるcase insensitiveな less()関数
bool CaseInsensitiveLess::operator() (const string& s1, const string& s2) const {

	return std::lexicographical_compare(s1.begin(), s1.end(), s2.begin(), s2.end(),
		[](char c1, char c2) { return tolower(c1) < tolower(c2); });
}

#if defined(__EMSCRIPTEN__) && defined(EVAL_NNUE)
// WASM NNUE
// 前回のOptions["EvalFile"]
std::string last_eval_file;
#endif

std::ostream& operator<<(std::ostream& os, const OptionsMap& om)
{
	// idxの順番を守って出力する
	for (size_t idx = 0; idx < om.size(); ++idx)
		for (const auto& it : om)
			if (it.second.idx == idx)
			{
				const Option& o = it.second;
				os << "option name " << it.first << " type " << o.type;

				if (o.type == "string" || o.type == "check" || o.type == "combo")
					os << " default " << o.defaultValue;

				if (o.type == "spin")
					// この範囲はStockfishではfloatになっているが、
					// やねうら王では、s64に変更する。
					os << " default " << s64(stoll(o.defaultValue))
					<< " min " << o.min
					<< " max " << o.max;

				// コンボボックス(やねうら王、独自追加)
				// USIで規定されている。
				if (o.list.size())
					for (auto v : o.list)
						os << " var " << v;

				// Stockfishはこの関数、最初に改行を放り込むように書いてあるけども、
				// 正直、使いにくいと思う。普通に末尾ごとに改行する。
				os << std::endl;

				break;
			}

	return os;
}

// --- Optionクラスのコンストラクタと変換子

Option::Option(const char* v, OnChange f) : type("string"), min(0), max(0), on_change(f)
{
	defaultValue = currentValue = v;
}

Option::Option(bool v, OnChange f) : type("check"), min(0), max(0), on_change(f)
{
	defaultValue = currentValue = (v ? "true" : "false");
}

Option::Option(OnChange f) : type("button"), min(0), max(0), on_change(f)
{
}

// Stockfishでは第一引数がdouble型だが、これは使わないと思うのでs64に変更する。
Option::Option(s64 v, s64 minv, s64 maxv, OnChange f) : type("spin"), min(minv), max(maxv), on_change(f)
{
	defaultValue = currentValue = std::to_string(v);
}

Option::Option(const std::vector<std::string>& list, const std::string& v, OnChange f)
	: type("combo"), on_change(f), list(list)
{
	defaultValue = currentValue = v;
}

Option::operator s64() const {
	ASSERT_LV1(type == "check" || type == "spin");
	return (type == "spin" ? std::stoll(currentValue) : currentValue == "true");
}

Option::operator std::string() const {
	//ASSERT_LV1(type == "string" || type == "combo" /* 将棋用拡張*/ );
	// →　string化して保存しておいた内容をあとで復元したいことがあるのでこのassertないほうがいい。
	// 代入しないとハンドラが起動しないので、そういう復元の仕方をしたいことがある。(ベンチマークなどで)
	return currentValue;
}

bool Option::operator==(const char* s) const {
	ASSERT_LV1(type == "combo");
	return    !CaseInsensitiveLess()(currentValue, s)
		&& !CaseInsensitiveLess()(s, currentValue);
}

// この関数はUSI::init()から起動時に呼び出されるだけ。
void Option::operator<<(const Option& o)
{
	static size_t insert_order = 0;
	*this = o;
	idx = insert_order++; // idxは生成順に0から連番で番号を振る
}

// USIプロトコル経由で値を設定されたときにそれをcurrentValueに反映させる。
Option& Option::operator=(const string& v) {

	ASSERT_LV1(!type.empty());

	// 範囲外なら設定せずに返る。
	// "EvalDir"などでstringの場合は空の文字列を設定したいことがあるので"string"に対して空の文字チェックは行わない。
	if (((type != "button" && type != "string") && v.empty())
		|| (type == "check" && v != "true" && v != "false")
		|| (type == "spin" && (stoll(v) < min || stoll(v) > max)))
		return *this;

	// ボタン型は値を設定するものではなく、単なるトリガーボタン。
	// ボタン型以外なら入力値をcurrentValueに反映させてやる。
	if (type != "button")
		currentValue = v;

	// 値が変化したのでハンドラを呼びだす。
	if (on_change)
		on_change(*this);

	return *this;
}

// --- 以下、やねうら王、独自拡張。

// idxの値を書き換えないoperator "<<"
void Option::overwrite(const Option& o)
{
	// 値が書き換わるのか？
	bool modified = this->currentValue != o.currentValue;

	// backup
	auto fn = this->on_change;
	auto idx_ = idx;

	*this = o;

	// restore
	idx = idx_;
	this->on_change = fn;

	// 値が書き換わったならハンドラを呼び出してやる。
	if (modified && fn)
		fn(*this);
}

// min = max = default = paramになる上書き
void Option::overwrite(const std::string& param)
{
	// 値が書き換わるのか？
	bool modified = this->currentValue != param;
	auto fn = this->on_change;

	if (modified)
	{
		this->currentValue = this->defaultValue = param;
		if (type == "spin")
			min = max = stoll(param);
		else if (type == "check")
			min = max = param == "true";
		else if (type == "combo")
		{
			list.clear();
			list.emplace_back(param);
		}
		// else if (type == "string")
		//	; // do_nothing
	}

	// 値が書き換わったならハンドラを呼び出してやる。
	if (modified && fn)
		fn(*this);
}


