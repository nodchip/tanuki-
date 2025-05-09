#ifndef USI_OPTION_H_INCLUDED
#define USI_OPTION_H_INCLUDED

#include <cstddef>
#include <functional>
#include <iosfwd>
#include <map>
#include <optional>
#include <string>

#include "extra/bitop.h"

class Option;

/// Define a custom comparator, because the UCI options should be case-insensitive
// UCIではオプションはcase insensitive(大文字・小文字の区別をしない)なのでcustom comparatorを用意する。
// USIではここがプロトコル上どうなっているのかはわからないが、同様の処理にしておく。
struct CaseInsensitiveLess {
	bool operator() (const std::string&, const std::string&) const;
};

/// The options container is defined as a std::map
// USIのoption名と、それに対応する設定内容を保持しているclass。実体はstd::map
using OptionsMap = std::map<std::string, Option, CaseInsensitiveLess>;

/// The Option class implements each option as specified by the UCI protocol
// USIプロトコルで指定されるoptionの内容を保持するclass
class Option {

	// USIプロトコルで"setoption"コマンドが送られてきたときに呼び出されるハンドラの型。
	//		typedef void(*OnChange)(const Option&);
	// Stockfishでは↑のように関数ポインタになっているが、
	// これだと[&](o){...}みたいなlambda式を受けられないのでここはstd::functionを使うべきだと思う。
	using OnChange = void (*)(const Option&);

public:
	// (GUI側のエンジン設定画面に出てくる)ボタン
	Option(OnChange f = nullptr);

	// 文字列
	Option(const char* v, OnChange f = nullptr);

	// (GUI側のエンジン設定画面に出てくる)CheckBox。bool型のoption デフォルト値が v
	Option(bool v, OnChange f = nullptr);

	// (GUI側のエンジン設定画面に出てくる)SpinBox。s64型。
	// Stockfishではdouble型になっているけども、GUI側がdoubleを受け付けるようになっていない可能性があるし、
	// doubleだと仮数部が52bitしかないので64bitの値を指定できなくて嫌だというのもある。
	// ゆえに、doubleはサポートせずにs64のみを扱う。
	Option(s64 v, s64 minv, s64 maxv, OnChange = nullptr);

	// (GUI側のエンジン設定画面に出てくる)ComboBox。内容的には、string型と同等。
	// list = コンボボックスに表示する値。v = デフォルト値かつ現在の値
	// StockfishにはComboBoxの取扱いがないようなのだが、これは必要だと思うのでやねうら王では独自に追加する。
	Option(const std::vector<std::string>& list, const std::string& v, OnChange f = nullptr);

	// USIプロトコル経由で値を設定されたときにそれをcurrentValueに反映させる。
	Option& operator=(const std::string&);

	// 起動時に設定を代入する。
	void operator<<(const Option&);

	// s64型への暗黙の変換子。
	// Stockfishでは、intになっているが、やねうら王ではs64に拡張している。
	operator s64() const;

	// string型への暗黙の変換子
	// typeが"string"型のとき以外であっても何であれ変換できるようになっているほうが便利なので
	// 変換できるようにしておく。
	operator std::string() const;

	// case insensitiveにしないといけないので比較演算子は独自に用意する。
	bool operator==(const char*) const;

	// idxの値を変えずに上書きする。
	// ※　やねうら王、独自拡張。
	// コマンド文字列からOptionのインスタンスを構築する時にこの機能が必要となる。
	void overwrite(const Option&);

	// 既存のOptionの上書き。
	// min = max = default = param になる。
	void overwrite(const std::string& param);


private:
	friend std::ostream& operator<<(std::ostream& os, const OptionsMap& om);

	std::string defaultValue, currentValue, type;

	// s64型のときの最小と最大
	// Stockfishではintになっているが、node limitなどs64の範囲の値を扱いたいのでやねうら王では拡張してある。
	s64 min, max;

	// 出力するときの順番。この順番に従ってGUIの設定ダイアログに反映されるので順番重要！
	size_t idx;

	// combo boxのときの表示する文字列リスト
	std::vector<std::string> list;

	// 値が変わったときに呼び出されるハンドラ
	OnChange on_change;
};

// USIプロトコルで、idxの順番でoptionを出力する。(デバッグ用)
std::ostream& operator<<(std::ostream& os, const OptionsMap& om);

#endif  // #ifndef USI_OPTION_H_INCLUDED
