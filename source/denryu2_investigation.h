#ifndef _DENRYU2_INVESTIGATION_H_
#define _DENRYU2_INVESTIGATION_H_

#include <sstream>

#include "usi.h"

class Position;

namespace Denryu2
{
	void Initialize(USI::OptionsMap& o);
	void ExtractTime();
	void ExtractGames();
	void CalculateMoveMatchRatio(Position& pos, std::istringstream& is);
	void CalculateMoveMatchRatio2(Position& pos);
}

#endif
