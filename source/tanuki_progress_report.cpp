#include "tanuki_progress_report.h"
#include "config.h"

#include <algorithm>
#include <cstdio>
#include <ctime>

namespace {
	double kAlpha = 0.1;
}

Tanuki::ProgressReport::ProgressReport(int64_t total_data, int64_t show_at_most_sec)
	: total_data_(total_data), show_at_most_sec_(show_at_most_sec), previous_data_(0),
	previous_time_(std::time(nullptr)), start_time_(std::time(nullptr)) {
}

void Tanuki::ProgressReport::Show(int64_t current_data) {
	if (current_data == 0) {
		return;
	}

	std::lock_guard<std::mutex> lock(show_mutex_);
	time_t current_time = std::time(nullptr);
	if (previous_time_ + show_at_most_sec_ > current_time) {
		return;
	}

	time_t elapsed_time = current_time - start_time_;
	int hour = static_cast<int>(elapsed_time / 3600);
	int minute = elapsed_time / 60 % 60;
	int second = elapsed_time % 60;

	time_t duration = current_time - previous_time_;
	exponential_moving_averaged_time_ =
		kAlpha * duration + (1.0 - kAlpha) * exponential_moving_averaged_time_;

	int64_t diff = current_data - previous_data_;
	exponential_moving_averaged_data_ =
		kAlpha * diff + (1.0 - kAlpha) * exponential_moving_averaged_data_;

	double time_per_data = exponential_moving_averaged_time_ / exponential_moving_averaged_data_;
	time_t expected_time =
		static_cast<time_t>(current_time + (total_data_ - current_data) * time_per_data);
	struct tm current_tm = *std::localtime(&current_time);
	struct tm expected_tm = *std::localtime(&expected_time);

	data_per_time_ = exponential_moving_averaged_data_ / exponential_moving_averaged_time_;
	max_data_per_time_ = std::max(max_data_per_time_, data_per_time_);
	has_data_per_time_ = true;

	std::printf(
		"%lld / %lld\n"
		"        elapsed time = %02d:%02d:%02d\n"
		"   current date time = %04d-%02d-%02d %02d:%02d:%02d\n"
		"    finish date time = %04d-%02d-%02d %02d:%02d:%02d\n"
		"               speed = %.2f (data/sec)\n"
		"           max speed = %.2f (data/sec)\n",
		current_data, total_data_,
		hour, minute, second,
		current_tm.tm_year + 1900, current_tm.tm_mon + 1, current_tm.tm_mday, current_tm.tm_hour, current_tm.tm_min, current_tm.tm_sec,
		expected_tm.tm_year + 1900, expected_tm.tm_mon + 1, expected_tm.tm_mday, expected_tm.tm_hour, expected_tm.tm_min, expected_tm.tm_sec,
		data_per_time_,
		max_data_per_time_);
	std::fflush(stdout);

	previous_data_ = current_data;
	previous_time_ = current_time;
}

void Tanuki::ProgressReport::Reset() {
	exponential_moving_averaged_data_ = 0.0;
	exponential_moving_averaged_time_ = 0.0;
	has_data_per_time_ = false;
}
