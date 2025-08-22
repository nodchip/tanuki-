#ifndef _TANUKI_SFEN_START_POSITION_PICKER_H_
#define _TANUKI_SFEN_START_POSITION_PICKER_H_

#include <random>

#include "config.h"
#include "position.h"
#include "tanuki_start_position_picker.h"

namespace Tanuki {
	/// <summary>
	/// SfenStartPositionPicker は 1 行に sfen が 1 つずつ書かれたファイルから開始局面を選択する。
	/// </summary>
	class SfenStartPositionPicker : public StartPositionPicker
	{
	public:
		virtual bool Open() override;
		virtual void Pick(Position& position, StateInfo*& state_info, Thread& thread) override;

	private:
		std::vector<std::string> start_positions_;
		std::vector<std::string>::iterator start_positions_iterator_;
		std::mutex mutex_;
	};
}

#endif
