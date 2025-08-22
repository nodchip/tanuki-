#ifndef _TANUKI_START_POSITION_PICKER_H_
#define _TANUKI_START_POSITION_PICKER_H_

#include "config.h"
#include "position.h"

namespace Tanuki {
	/// <summary>
	/// StartPositionPicker は、自己対局の開始局面を選択するためのクラスの基底クラスです。
	/// </summary>
	class StartPositionPicker
	{
	public:
		virtual ~StartPositionPicker() = default;

		/// <summary>
		/// 開始局面の元データを開く
		/// </summary>
		/// <returns>成功したら true、そうでない場合は false</returns>
		virtual bool Open() = 0;

		/// <summary>
		/// 開始局面を 1 局面選択する。
		/// <para>複数スレッドから同時に呼ばれるため、内部的にロックを取ることが要求される。</para>
		/// </summary>
		/// <param name="position"></param>
		/// <param name="state_info">StateInfoへのポインターの参照。Pick()が呼ばれたあと、未使用のStateInfoまでインクリメントされる。</param>
		/// <param name="thread"></param>
		virtual void Pick(Position& position, StateInfo*& state_info, Thread& thread) = 0;
	};
}

#endif
