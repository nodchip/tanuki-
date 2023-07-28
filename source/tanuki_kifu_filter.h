#ifndef _TANUKI_KIFU_FILTER_H_
#define _TANUKI_KIFU_FILTER_H_

#ifdef EVAL_LEARN

#include "usi.h"

namespace Tanuki {
	void InitializeFilter(USI::OptionsMap& o);
	void FilterBishopExchange();
}

#endif

#endif
