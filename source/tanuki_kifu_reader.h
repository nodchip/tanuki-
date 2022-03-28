#ifndef _TANUKI_KIFU_READER_H_
#define _TANUKI_KIFU_READER_H_

#include "config.h"

#ifdef EVAL_LEARN

#include <fstream>
#include <mutex>
#include <string>
#include <vector>

#include "learn/learn.h"
#include "usi.h"

namespace Tanuki {
	class KifuReader {
	public:
		KifuReader(const std::string& folder_name, int num_loops);
		virtual ~KifuReader();
		bool Read(Learner::PackedSfenValue& record);
		bool Read(int num_records, std::vector<Learner::PackedSfenValue>& records);
		bool Close();
		static void Initialize(USI::OptionsMap& o);

	private:
		bool EnsureOpen();

		const std::string folder_name_;
		const int num_loops_;
		std::vector<std::string> file_paths_;
		FILE* file_;
		int loop_ = 0;
		int file_index_ = 0;
	};
}

#endif

#endif
