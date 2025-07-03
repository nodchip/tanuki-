#ifndef _TANUKI_TRAINING_DATA_H_INCLUDED_
#define _TANUKI_TRAINING_DATA_H_INCLUDED_

#include <istream>

namespace Tanuki
{
	void Rescore(std::istringstream& is);
	void Ensemble(std::istringstream& is);
	void CopyMateValue();
	void Unique();
}

#endif
