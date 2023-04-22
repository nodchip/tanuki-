#ifndef _TANUKI_KIFU_GENERATOR_H_
#define _TANUKI_KIFU_GENERATOR_H_

#include "config.h"

#ifdef EVAL_LEARN

#include "usi.h"

namespace Tanuki {
	void InitializeGenerator(USI::OptionsMap& o);
	void GenerateKifu();
	void ConvertSfenToLearningData();
	void FilterKifu();
}

#endif

#endif
