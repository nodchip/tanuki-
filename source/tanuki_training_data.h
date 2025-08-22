#ifndef _TANUKI_TRAINING_DATA_H_INCLUDED_
#define _TANUKI_TRAINING_DATA_H_INCLUDED_

#include <istream>

#include "usi.h"

namespace Tanuki
{
	// source\learn\learn.hよりコピー
	// PackedSfenと評価値が一体化した構造体
	// オプションごとに書き出す内容が異なると教師棋譜を再利用するときに困るので
	// とりあえず、以下のメンバーはオプションによらずすべて書き出しておく。
	struct PackedSfenValue
	{
		// 局面
		PackedSfen sfen;

		// Learner::search()から返ってきた評価値
		s16 score;

		// PVの初手
		// 教師との指し手一致率を求めるときなどに用いる
		u16 move;

		// 初期局面からの局面の手数。
		u16 gamePly;

		// この局面の手番側が、ゲームを最終的に勝っているなら1。負けているなら-1。
		// 引き分けに至った場合は、0。
		// 引き分けは、教師局面生成コマンドgensfenにおいて、
		// LEARN_GENSFEN_DRAW_RESULTが有効なときにだけ書き出す。
		s8 game_result;

		// 教師局面を書き出したファイルを他の人とやりとりするときに
		// この構造体サイズが不定だと困るため、paddingしてどの環境でも必ず40bytesになるようにしておく。
		u8 padding;

		// 32 + 2 + 2 + 2 + 1 + 1 = 40bytes

		bool operator<(const PackedSfenValue& rh) const {
			return std::memcmp(sfen.data, rh.sfen.data, sizeof(sfen.data)) < 0;
		}
	};

	void InitializeGenerator(USI::OptionsMap& o);
	void Rescore(std::istringstream& is);
	void Ensemble(std::istringstream& is);
	void CopyMateValue();
	void Unique();
	void Generate();
}

#endif
