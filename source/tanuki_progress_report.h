#ifndef _TANUKI_PROGRESS_REPORT_H_
#define _TANUKI_PROGRESS_REPORT_H_

#include "config.h"

#include <cstdint>
#include <mutex>

namespace Tanuki {
	class ProgressReport {
	public:
		ProgressReport(int64_t total_data, int64_t show_at_most_sec);
		void Show(int64_t current_data);
		bool HasDataPerTime() const { return has_data_per_time_; }
		double GetDataPerTime() const { return data_per_time_; }
		double GetMaxDataPerTime() const { return max_data_per_time_; }
		void Reset();

	private:
		const int64_t total_data_;
		const int64_t show_at_most_sec_;
		int64_t previous_data_ = 0;
		time_t start_time_;
		time_t previous_time_;
		double exponential_moving_averaged_data_ = 0.0;
		double exponential_moving_averaged_time_ = 0.0;
		std::mutex show_mutex_;
		double data_per_time_;
		double max_data_per_time_ = 0.0;
		bool has_data_per_time_ = false;
	};
}

#endif
