#include "tanuki_training_data.h"

#include <cstdio>
#include <ctime>
#include <filesystem>
#include <iomanip>
#include <random>
#include <sstream>
#include <string>

#include "engine/dlshogi-engine/dlshogi_searcher.h"
#include "engine/dlshogi-engine/UctSearch.h"
#include "eval/deep/nn.h"
#include "eval/deep/nn_types.h"
#include "position.h"
#include "tanuki_kifu_writer.h"
#include "tanuki_progress_report.h"
#include "tanuki_sfen_start_position_picker.h"
#include "thread.h"

namespace
{
	static constexpr size_t BUFFER_SIZE = 1024 * 1024 * 1024;
	static constexpr int kMaxGamePlay = 400;
	static constexpr time_t kInitialRescoreReportIntervalSec = 1;
	static constexpr time_t kMaxRescoreReportIntervalSec = 60 * 60;
	static constexpr double kRescoreReportAlpha = 0.2;

	template <typename T>
	T ParseOptionOrDie(const char* name) {
		std::string value_string = (std::string)Options[name];
		std::istringstream iss(value_string);
		T value;
		if (!(iss >> value)) {
			sync_cout << "Failed to parse an option. Exitting...: name=" << name << " value=" << value
				<< sync_endl;
			std::exit(1);
		}
		return value;
	}

	enum GameResult {
		GameResultWin = 1,
		GameResultLose = -1,
		GameResultDraw = 0,
	};

	std::string FormatDuration(time_t seconds) {
		if (seconds < 0) {
			seconds = 0;
		}

		const int hour = static_cast<int>(seconds / 3600);
		const int minute = static_cast<int>((seconds / 60) % 60);
		const int second = static_cast<int>(seconds % 60);

		char buffer[32];
		std::snprintf(buffer, sizeof(buffer), "%02d:%02d:%02d", hour, minute, second);
		return buffer;
	}

	std::string FormatTimestamp(time_t timestamp) {
		std::tm local_time = {};
		localtime_s(&local_time, &timestamp);

		char buffer[32];
		std::snprintf(
			buffer,
			sizeof(buffer),
			"%04d-%02d-%02d %02d:%02d:%02d",
			local_time.tm_year + 1900,
			local_time.tm_mon + 1,
			local_time.tm_mday,
			local_time.tm_hour,
			local_time.tm_min,
			local_time.tm_sec);
		return buffer;
	}

	std::string FormatRate(double records_per_second) {
		const char* suffixes[] = { "rec/s", "K rec/s", "M rec/s", "G rec/s" };
		size_t suffix_index = 0;
		while (1000.0 <= records_per_second && suffix_index + 1 < _countof(suffixes)) {
			records_per_second /= 1000.0;
			++suffix_index;
		}

		char buffer[64];
		if (suffix_index == 0) {
			std::snprintf(buffer, sizeof(buffer), "%.0f %s", records_per_second, suffixes[suffix_index]);
		}
		else {
			std::snprintf(buffer, sizeof(buffer), "%.1f %s", records_per_second, suffixes[suffix_index]);
		}
		return buffer;
	}

	class RescoreProgressReporter {
	public:
		explicit RescoreProgressReporter(int64_t total_records)
			: total_records_(total_records),
			start_time_(std::time(nullptr)),
			last_report_time_(start_time_),
			last_reported_records_(0),
			next_interval_sec_(kInitialRescoreReportIntervalSec) {
		}

		void MaybeReport(int64_t processed_records) {
			const time_t now = std::time(nullptr);
			if (now < last_report_time_ + next_interval_sec_) {
				return;
			}

			Report(processed_records, now, false);
			next_interval_sec_ =
				std::min(next_interval_sec_ * 2, kMaxRescoreReportIntervalSec);
		}

		void ReportFinal(int64_t processed_records) {
			Report(processed_records, std::time(nullptr), true);
		}

	private:
		void Report(int64_t processed_records, time_t now, bool is_final) {
			if (processed_records < 0) {
				processed_records = 0;
			}
			if (total_records_ < processed_records) {
				processed_records = total_records_;
			}

			const time_t elapsed_sec = std::max<time_t>(now - start_time_, 0);
			const time_t duration_since_last_report =
				std::max<time_t>(now - last_report_time_, 1);
			const int64_t records_since_last_report =
				std::max<int64_t>(processed_records - last_reported_records_, 0);
			const double current_rate =
				static_cast<double>(records_since_last_report) / duration_since_last_report;
			if (!has_smoothed_rate_) {
				smoothed_rate_ = current_rate;
				has_smoothed_rate_ = 0.0 < current_rate;
			}
			else if (0.0 < current_rate) {
				smoothed_rate_ =
					kRescoreReportAlpha * current_rate + (1.0 - kRescoreReportAlpha) * smoothed_rate_;
			}

			double progress_percent = 100.0;
			if (0 < total_records_) {
				progress_percent =
					100.0 * static_cast<double>(processed_records) / static_cast<double>(total_records_);
			}

			time_t eta_time = now;
			if (has_smoothed_rate_ && processed_records < total_records_) {
				const double remaining_records =
					static_cast<double>(total_records_ - processed_records);
				eta_time += static_cast<time_t>(remaining_records / smoothed_rate_);
			}

			std::ostringstream oss;
			oss
				<< "info string Rescore "
				<< processed_records << "/" << total_records_
				<< " (" << std::fixed << std::setprecision(1) << progress_percent
				<< std::defaultfloat << "%) ";
			if (has_smoothed_rate_) {
				oss << FormatRate(smoothed_rate_);
			}
			else {
				oss << "warming up";
			}

			oss
				<< " elapsed " << FormatDuration(elapsed_sec)
				<< " ETA " << FormatTimestamp(eta_time);
			if (!is_final) {
				oss << " next report in " << FormatDuration(next_interval_sec_);
			}
			sync_cout << oss.str() << sync_endl;

			last_report_time_ = now;
			last_reported_records_ = processed_records;
		}

		const int64_t total_records_;
		const time_t start_time_;
		time_t last_report_time_;
		int64_t last_reported_records_;
		time_t next_interval_sec_;
		double smoothed_rate_ = 0.0;
		bool has_smoothed_rate_ = false;
	};
}

using dlshogi::UctSearcherGroup;
using dlshogi::DlshogiSearcher;
using dlshogi::UctSearcher;
using Eval::dlshogi::NN;
using Eval::dlshogi::PType;
using Eval::dlshogi::NN_Input1;
using Eval::dlshogi::NN_Input2;
using Eval::dlshogi::NN_Output_Policy;
using Eval::dlshogi::NN_Output_Value;
using USI::Option;
using USI::OptionsMap;

extern DlshogiSearcher searcher;

constexpr const char* kOptionKifuDir = "KifuDir";
constexpr const char* kOptionGeneratorNumPositions = "GeneratorNumPositions";
constexpr const char* kOptionGeneratorKifuTag = "GeneratorKifuTag";
constexpr const char* kOptionGeneratorStartposFileName = "GeneratorStartposFileName";
constexpr const char* kOptionGeneratorStartPositionMaxPlay = "GeneratorStartPositionMaxPlay";

void Tanuki::InitializeGenerator(USI::OptionsMap& o) {
	o[kOptionKifuDir] << Option("");
	o[kOptionGeneratorNumPositions] << Option("10000000000");
	o[kOptionGeneratorKifuTag] << Option("default_tag");
	o[kOptionGeneratorStartposFileName] << Option("startpos.sfen");
	o[kOptionGeneratorStartPositionMaxPlay] << Option(std::numeric_limits<int>::max(), 1, std::numeric_limits<int>::max());
}

void Tanuki::Rescore(std::istringstream& is)
{
	static constexpr int batch_size = 1024;

	Options["DNN_Batch_Size1"] = std::to_string(batch_size);

	is_ready();

	std::string input_file_path;
	std::string output_file_path;
	is >> input_file_path >> output_file_path;
	int gpu_id = 0;

	UctSearcherGroup& grp = searcher.GetSearchGroups()[0];
	UctSearcher* searcher = grp.get_uct_searcher(0);

	PType* packed_features1 = searcher->packed_features1;
	PType* packed_features2 = searcher->packed_features2;
	NN_Input1* features1 = searcher->features1;
	NN_Input2* features2 = searcher->features2;

	NN_Output_Policy* y1 = searcher->y1;
	NN_Output_Value* y2 = searcher->y2;

	FILE* input_file = std::fopen(input_file_path.c_str(), "rb");
	if (input_file == nullptr) {
		sync_cout << "info string Rescore failed to open input file: "
			<< input_file_path << sync_endl;
		return;
	}
	std::setvbuf(input_file, nullptr, _IOFBF, BUFFER_SIZE);

	FILE* output_file = std::fopen(output_file_path.c_str(), "wb");
	if (output_file == nullptr) {
		sync_cout << "info string Rescore failed to open output file: "
			<< output_file_path << sync_endl;
		std::fclose(input_file);
		return;
	}
	std::setvbuf(output_file, nullptr, _IOFBF, BUFFER_SIZE);

	_fseeki64(input_file, 0, SEEK_END);
	const int64_t input_file_size = _ftelli64(input_file);
	_fseeki64(input_file, 0, SEEK_SET);
	if (input_file_size < 0
		|| input_file_size % static_cast<int64_t>(sizeof(PackedSfenValue)) != 0) {
		sync_cout << "info string Rescore failed: invalid input file size: "
			<< input_file_size << sync_endl;
		std::fclose(output_file);
		std::fclose(input_file);
		return;
	}

	const int64_t total_records =
		input_file_size / static_cast<int64_t>(sizeof(PackedSfenValue));
	if (total_records == 0) {
		sync_cout << "info string Rescore finished 0/0 (empty input)" << sync_endl;
		std::fclose(output_file);
		std::fclose(input_file);
		return;
	}

	// Show progress quickly at startup, then back off to keep Jenkins logs compact.
	RescoreProgressReporter progress_reporter(total_records);
	int64_t num_processed = 0;
	std::vector<PackedSfenValue> packed_sfens(batch_size);
	for (;;) {
		size_t num_samples = std::fread(&packed_sfens[0], sizeof(PackedSfenValue), batch_size, input_file);
		if (num_samples == 0) {
			break;
		}

		for (int sample_index = 0; sample_index < num_samples; ++sample_index) {
			Position& position = Threads.main()->rootPos;
			StateInfo state_info;
			const auto result =
				position.set_from_packed_sfen(packed_sfens[sample_index].sfen, &state_info, Threads.main());
			if (result.is_not_ok()) {
				sync_cout << "info string Rescore failed: invalid packed sfen at sample "
					<< sample_index << " result " << result.to_string() << sync_endl;
				std::fclose(output_file);
				std::fclose(input_file);
				return;
			}
			Eval::dlshogi::make_input_features(position, sample_index, packed_features1, packed_features2);
		}

		grp.nn_forward(batch_size, packed_features1, packed_features2, features1, features2, y1, y2);

		for (int sample_index = 0; sample_index < num_samples; ++sample_index) {
			DType p = y2[sample_index];
			p = std::clamp(p, 1e-5f, 1.0f - 1e-5f);
			DType value = -600.0f * std::log((1.0f - p) / p);
			value = std::clamp(value, static_cast<DType>(VALUE_MIN_EVAL), static_cast<DType>(VALUE_MAX_EVAL));
			packed_sfens[sample_index].score = static_cast<s16>(value);
		}

		std::fwrite(&packed_sfens[0], sizeof(PackedSfenValue), num_samples, output_file);

		num_processed += num_samples;
		progress_reporter.MaybeReport(num_processed);
	}

	progress_reporter.ReportFinal(num_processed);
	sync_cout << "info string Rescore finished." << sync_endl;

	std::fclose(output_file);
	output_file = nullptr;

	std::fclose(input_file);
	input_file = nullptr;
}

void Tanuki::Ensemble(std::istringstream& is)
{
	static constexpr int batch_size = 1024 * 1024;

	std::vector<std::string> file_paths;
	std::string file_path;
	while (is >> file_path) {
		file_paths.push_back(file_path);
	}

	std::vector<std::string> input_file_paths(file_paths.begin(), file_paths.end() - 1);
	std::string output_file_path = file_paths.back();

	std::vector<FILE*> input_files;
	int64_t total_records = -1;
	for (const auto& input_file_path : input_file_paths) {
		FILE* input_file = std::fopen(input_file_path.c_str(), "rb");
		if (input_file == nullptr) {
			sync_cout << "info string Ensemble failed to open input file: "
				<< input_file_path << sync_endl;
			for (auto& opened_input_file : input_files) {
				std::fclose(opened_input_file);
				opened_input_file = nullptr;
			}
			return;
		}
		input_files.push_back(input_file);
		std::setvbuf(input_files.back(), nullptr, _IOFBF, BUFFER_SIZE);

		_fseeki64(input_files.back(), 0, SEEK_END);
		const int64_t input_file_size = _ftelli64(input_files.back());
		_fseeki64(input_files.back(), 0, SEEK_SET);
		if (input_file_size < 0
			|| input_file_size % static_cast<int64_t>(sizeof(PackedSfenValue)) != 0) {
			sync_cout << "info string Ensemble failed: invalid input file size: "
				<< input_file_path << " size=" << input_file_size << sync_endl;
			for (auto& opened_input_file : input_files) {
				std::fclose(opened_input_file);
				opened_input_file = nullptr;
			}
			return;
		}

		const int64_t current_total_records =
			input_file_size / static_cast<int64_t>(sizeof(PackedSfenValue));
		if (total_records < 0) {
			total_records = current_total_records;
		}
		else if (total_records != current_total_records) {
			sync_cout << "info string Ensemble failed: input sizes do not match: "
				<< input_file_path << " records=" << current_total_records
				<< " expected=" << total_records << sync_endl;
			for (auto& opened_input_file : input_files) {
				std::fclose(opened_input_file);
				opened_input_file = nullptr;
			}
			return;
		}
	}

	FILE* output_file = std::fopen(output_file_path.c_str(), "wb");
	if (output_file == nullptr) {
		sync_cout << "info string Ensemble failed to open output file: "
			<< output_file_path << sync_endl;
		for (auto& input_file : input_files) {
			std::fclose(input_file);
			input_file = nullptr;
		}
		return;
	}
	std::setvbuf(output_file, nullptr, _IOFBF, BUFFER_SIZE);

	if (total_records == 0) {
		sync_cout << "info string Ensemble finished 0/0 (empty input)" << sync_endl;
		std::fclose(output_file);
		output_file = nullptr;
		for (auto& input_file : input_files) {
			std::fclose(input_file);
			input_file = nullptr;
		}
		return;
	}

	RescoreProgressReporter progress_reporter(total_records);
	int64_t num_processed = 0;
	int num_input_files = static_cast<int>(input_file_paths.size());
	std::vector<std::vector<PackedSfenValue>> input_packed_sfens(
		num_input_files, std::vector<PackedSfenValue>(batch_size));
	for (;;) {
		size_t min_num_samples = std::numeric_limits<size_t>::max();
		for (int input_file_index = 0; input_file_index < static_cast<int>(num_input_files);
			++input_file_index) {
			size_t num_samples = std::fread(
				&input_packed_sfens[input_file_index][0], sizeof(PackedSfenValue), batch_size,
				input_files[input_file_index]);
			min_num_samples = std::min(min_num_samples, num_samples);
		}
		if (min_num_samples == 0) {
			break;
		}

		std::vector<PackedSfenValue> output_packed_sfens = input_packed_sfens[0];
		for (int position_index = 0; position_index < static_cast<int>(min_num_samples);
			++position_index) {
			int sum_scores = 0;
			for (int input_file_index = 0; input_file_index < static_cast<int>(num_input_files);
				++input_file_index) {
				sum_scores += input_packed_sfens[input_file_index][position_index].score;
			}

			output_packed_sfens[position_index].score = sum_scores / num_input_files;
		}

		std::fwrite(&output_packed_sfens[0], sizeof(PackedSfenValue), min_num_samples, output_file);

		num_processed += min_num_samples;
		progress_reporter.MaybeReport(num_processed);
	}

	progress_reporter.ReportFinal(num_processed);
	sync_cout << "info string Ensemble finished." << sync_endl;

	std::fclose(output_file);
	output_file = nullptr;

	for (auto& input_file : input_files) {
		std::fclose(input_file);
		input_file = nullptr;
	}
}

void Tanuki::CopyMateValue()
{
	static constexpr int batch_size = 1024 * 1024;

	std::string input_file_path = R"(D:\hnoda\shogi\training_data\tanuki-.nnue-pytorch-2024-07-30.1.shuffled\shuffled.bin)";
	std::string distilled_file_path = R"(D:\hnoda\shogi\training_data\tanuki-.nnue-pytorch-2024-07-30.1.shuffled\shuffled.bin)";
	std::string output_file_path = R"(D:\hnoda\shogi\training_data\tanuki-.nnue-pytorch-2024-07-30.1.shuffled\shuffled.mate.bin)";

	FILE* input_file = std::fopen(input_file_path.c_str(), "rb");
	std::setvbuf(input_file, nullptr, _IOFBF, BUFFER_SIZE);

	FILE* distilled_file = std::fopen(distilled_file_path.c_str(), "rb");
	std::setvbuf(distilled_file, nullptr, _IOFBF, BUFFER_SIZE);

	FILE* output_file = std::fopen(output_file_path.c_str(), "wb");
	std::setvbuf(output_file, nullptr, _IOFBF, BUFFER_SIZE);

	int64_t num_processed = 0;
	int64_t progress_duration = 1000000;
	int64_t next_progress = progress_duration;
	std::vector<PackedSfenValue> input_packed_sfens(batch_size);
	std::vector<PackedSfenValue> distilled_packed_sfens(batch_size);
	while (!std::feof(input_file)) {
		size_t min_num_samples = std::numeric_limits<size_t>::max();
		size_t input_samples =
			std::fread(&input_packed_sfens[0], sizeof(PackedSfenValue), batch_size, input_file);
		size_t distilled_samples =
			std::fread(&distilled_packed_sfens[0], sizeof(PackedSfenValue), batch_size, input_file);
		min_num_samples = std::min(min_num_samples, distilled_samples);

		std::vector<PackedSfenValue> output_packed_sfens = distilled_packed_sfens;
		for (int position_index = 0; position_index < static_cast<int>(min_num_samples);
			++position_index) {
			if (VALUE_MATE_IN_MAX_PLY <= std::abs(input_packed_sfens[position_index].score)) {
				output_packed_sfens[position_index].score = input_packed_sfens[position_index].score;
			}
		}

		std::fwrite(&output_packed_sfens[0], sizeof(PackedSfenValue), min_num_samples, output_file);

		num_processed += min_num_samples;
		if (next_progress < num_processed) {
			std::cout << num_processed << std::endl;
			next_progress += progress_duration;
		}
	}

	std::cout << "Finished." << std::endl;

	std::fclose(output_file);
	output_file = nullptr;

	std::fclose(distilled_file);
	distilled_file = nullptr;

	std::fclose(input_file);
	input_file = nullptr;
}

void Tanuki::Unique()
{
	static constexpr int batch_size = 1024 * 1024;
	static constexpr int large_buffer_size = 1024 * 1024 * 1024;
	//static constexpr int small_buffer_size = 256 * 1024 * 1024;
	static constexpr int small_buffer_size = 128 * 1024 * 1024;
	static constexpr int num_files = 256;

	std::string input_file_path = R"(D:\hnoda\shogi\training_data\tanuki-.nnue-pytorch-2024-07-30.1.shuffled\shuffled.bin)";
	std::string temp_folder_path = R"(D:\hnoda\shogi\training_data\tanuki-.nnue-pytorch-2024-07-30.1.shuffled)";
	std::string output_file_path = R"(D:\hnoda\shogi\training_data\tanuki-.nnue-pytorch-2024-07-30.1.shuffled\shuffled.unique.bin)";

	// 棋譜を入力し、複数のファイルにランダムに追加していく
	FILE* input_file = std::fopen(input_file_path.c_str(), "rb");
	std::setvbuf(input_file, nullptr, _IOFBF, large_buffer_size);

	std::vector<std::string> temp_file_paths;
	for (int file_index = 0; file_index < num_files; ++file_index) {
		std::string file_path = temp_folder_path + "\\" + std::to_string(file_index) + ".bin";
		temp_file_paths.push_back(file_path);
	}

	sync_cout << "info string Opening temp output files..." << sync_endl;
	std::vector<FILE*> temp_files;
	for (const auto& file_path : temp_file_paths) {
		FILE* file = std::fopen(file_path.c_str(), "wb");
		std::setvbuf(input_file, nullptr, _IOFBF, small_buffer_size);
		temp_files.push_back(file);
	}

	sync_cout << "info string Starting dividing..." << sync_endl;

	std::mt19937_64 mt(std::time(nullptr));
	std::uniform_int_distribution<> dist(0, num_files - 1);
	int64_t num_records = 0;

	for (;;) {
		std::vector<PackedSfenValue> records(batch_size);
		int num_samples = std::fread(&records[0], sizeof(records[0]), batch_size, input_file);

		if (num_samples == 0) {
			break;
		}

		for (int sample_index = 0; sample_index < num_samples; ++sample_index) {
			std::fwrite(&records[sample_index], sizeof(records[sample_index]), 1, temp_files[dist(mt)]);
			++num_records;
			if (num_records % 10000000 == 0) {
				sync_cout << "info string " << num_records << sync_endl;
			}
		}
	}
	for (auto temp_file : temp_files) {
		std::fclose(temp_file);
	}
	temp_files.clear();

	// 分割した学習データをソートする。
	sync_cout << "info string Starting shuffling..." << sync_endl;

	for (const auto& temp_file_path : temp_file_paths) {
		sync_cout << "info string " << temp_file_path << sync_endl;

		FILE* temp_file = std::fopen(temp_file_path.c_str(), "rb");
		std::setvbuf(temp_file, nullptr, _IOFBF, large_buffer_size);

		_fseeki64(temp_file, 0, SEEK_END);
		int64_t file_size = _ftelli64(temp_file);
		_fseeki64(temp_file, 0, SEEK_SET);
		std::vector<PackedSfenValue> records(file_size / sizeof(PackedSfenValue));
		std::fread(&records[0], sizeof(PackedSfenValue), file_size / sizeof(PackedSfenValue), temp_file);
		std::fclose(temp_file);
		temp_file = nullptr;

		std::sort(records.begin(), records.end());

		temp_file = std::fopen(temp_file_path.c_str(), "wb");
		std::setvbuf(temp_file, nullptr, _IOFBF, large_buffer_size);
		std::fwrite(&records[0], sizeof(PackedSfenValue), records.size(), temp_file);
		std::fclose(temp_file);
		temp_file = nullptr;
	}

	sync_cout << "info string Starting merging..." << sync_endl;

	std::priority_queue<std::pair<PackedSfenValue, FILE*>, std::vector<std::pair<PackedSfenValue, FILE*>>, std::greater<>> q;

	for (const auto& temp_file_path : temp_file_paths) {
		sync_cout << "info string " << temp_file_path << sync_endl;

		FILE* temp_file = std::fopen(temp_file_path.c_str(), "rb");
		std::setvbuf(temp_file, nullptr, _IOFBF, small_buffer_size);

		PackedSfenValue packed_sfen_value;
		std::fread(&packed_sfen_value, sizeof(packed_sfen_value), 1, temp_file);

		q.emplace(packed_sfen_value, temp_file);
	}

	num_records = 0;
	int64_t num_duplicated = 0;

	FILE* output_file = std::fopen(output_file_path.c_str(), "wb");
	std::setvbuf(output_file, nullptr, _IOFBF, large_buffer_size);
	PackedSfenValue last_packed_sfen_value = {};
	while (!q.empty()) {
		auto [packed_sfen_value, file] = q.top();
		q.pop();

		if (std::memcmp(&last_packed_sfen_value.sfen, &packed_sfen_value.sfen,
			sizeof(last_packed_sfen_value.sfen)) != 0) {
			std::fwrite(&packed_sfen_value, sizeof(packed_sfen_value), 1, output_file);
		}
		else {
			++num_duplicated;
		}
		last_packed_sfen_value = packed_sfen_value;

		if (std::fread(&packed_sfen_value, sizeof(packed_sfen_value), 1, file)) {
			q.emplace(packed_sfen_value, file);
		}
		else {
			std::fclose(file);
			file = nullptr;
		}

		++num_records;
		if (num_records % 10000000 == 0) {
			sync_cout << "info string " << num_records << sync_endl;
		}
	}

	sync_cout << "info string " << (num_duplicated * 100.0 / num_records) << "% duplicated." << sync_endl;

	std::fclose(output_file);
	output_file = nullptr;

	for (const auto& temp_file_path : temp_file_paths) {
		std::filesystem::remove(temp_file_path);
	}
}

void Tanuki::Generate()
{
	constexpr int batch_size = 1024;

	Options["DNN_Batch_Size1"] = std::to_string(batch_size);

	is_ready();

	Search::clear();

	std::srand(static_cast<unsigned int>(std::time(nullptr)));

	// 開始局面集を読み込む。
	std::unique_ptr<StartPositionPicker> start_position_picker(new SfenStartPositionPicker());
	if (!start_position_picker->Open()) {
		return;
	}

	std::string kifu_directory = (std::string)Options["KifuDir"];
	std::filesystem::create_directories(kifu_directory);

	int64_t num_positions = ParseOptionOrDie<int64_t>(kOptionGeneratorNumPositions);
	std::string output_file_name_tag = Options[kOptionGeneratorKifuTag];

	std::cout << "num_positions=" << num_positions << std::endl;
	std::cout << "output_file_name_tag=" << output_file_name_tag << std::endl;

	time_t start_time;
	std::time(&start_time);

	// スレッド間で共有する
	std::atomic_int64_t global_position_index;
	global_position_index = 0;
	ProgressReport progress_report(num_positions, 60 * 60);
	//ProgressReport progress_report(num_positions, 60);
	//ProgressReport progress_report(num_positions, 1);
	std::atomic<int> num_records = 0;
	char output_file_path[1024];
	std::sprintf(output_file_path,
		"%s/kifu.tag=%s.num_positions=%I64d.start_time=%I64d.bin",
		kifu_directory.c_str(), output_file_name_tag.c_str(), num_positions,
		start_time);
	std::unique_ptr<KifuWriter> kifu_writer = std::make_unique<KifuWriter>(output_file_path);

	std::mt19937_64 mt19937_64(start_time);
	std::uniform_real_distribution<float> move_distribution(0.0f, 1.0f);

	struct GameState {
		Position pos;
		std::vector<StateInfo> state_info = std::vector<StateInfo>(1024);
		StateInfo* state_info_ptr = &state_info[0];
		std::vector<PackedSfenValue> records;
	};
	std::vector<GameState> game_states(batch_size);
	// 初期局面を選択する。
	for (auto& state : game_states) {
		start_position_picker->Pick(state.pos, state.state_info_ptr, *Threads.main());
	}

	// ふかうら王から引っ張ってくるもの。
	UctSearcherGroup& grp = searcher.GetSearchGroups()[0];
	UctSearcher* searcher = grp.get_uct_searcher(0);

	PType* packed_features1 = searcher->packed_features1;
	PType* packed_features2 = searcher->packed_features2;
	NN_Input1* features1 = searcher->features1;
	NN_Input2* features2 = searcher->features2;

	NN_Output_Policy* y1 = searcher->y1;
	NN_Output_Value* y2 = searcher->y2;

	std::vector<float> legal_move_probabilities;

	int num_games = 0;
	int64_t sum_plays = 0;
	while (global_position_index < num_positions) {
		for (int sample_index = 0; sample_index < batch_size; ++sample_index) {
			Eval::dlshogi::make_input_features(
				game_states[sample_index].pos, sample_index, packed_features1, packed_features2);
		}

		grp.nn_forward(batch_size, packed_features1, packed_features2, features1, features2, y1, y2);

		for (int sample_index = 0; sample_index < batch_size; ++sample_index) {
			GameState& game_state = game_states[sample_index];
			Position& pos = game_state.pos;

			if (
				// 一定の手数に達していない
				pos.game_ply() < kMaxGamePlay &&
				// 詰まされていない
				!pos.is_mated() &&
				// 宣言勝ちができない
				pos.DeclarationWin() == Move::none() &&
				// 千日手による引き分けではない
				// 優等局面・劣等局面は、対局中に一瞬だけ現れ、その後通常通り対局が進むパターンがあるため、考慮しない
				pos.is_repetition() != RepetitionState::REPETITION_DRAW) {
				// 次の指し手を決める。
				Move selected_move = Mate::mate_1ply(pos);
				if (selected_move == Move::none())
				{
					// 1手詰めではない場合、ニューラルネットワークの出力から指し手を選ぶ。
					MoveList<LEGAL> move_list(game_state.pos);
					legal_move_probabilities.clear();
					for (ExtMove move : move_list) {
						int move_label = make_move_label(Move(move), pos.side_to_move());
						float probability = y1[sample_index][move_label];
						legal_move_probabilities.push_back(probability);
					}

					// Boltzmann distribution
					softmax_temperature_with_normalize(legal_move_probabilities);

					//sync_cout << pos << sync_endl;

					float max_probability = *std::max_element(
						legal_move_probabilities.begin(), legal_move_probabilities.end());
					float min_probability_threshold = 0.1;
					while (max_probability < min_probability_threshold) {
						min_probability_threshold *= 0.5f;
					}

					// 指し手を選ぶ。
					do {
						float rand_probability = move_distribution(mt19937_64);
						float cumulative_probability = 0.0f;
						for (int move_index = 0; move_index < move_list.size(); ++move_index) {
							cumulative_probability += legal_move_probabilities[move_index];
							//sync_cout << *move << " " << legal_move_probabilities[move - moves] << sync_endl;

							if (cumulative_probability >= rand_probability) {
								if (min_probability_threshold <= legal_move_probabilities[move_index]) {
									selected_move = static_cast<Move>(move_list.at(move_index));
								}
								break;
							}
						}
					} while (selected_move == Move::none());
				}

				if (selected_move == Move::none()) {
					// 何らかの理由で合法手が見つからなかった。
					sync_cout << "info string No legal move found."
						<< " sample_index=" << sample_index << std::endl
						<< pos << sync_endl;

					MoveList<LEGAL> move_list(game_state.pos);
					for (ExtMove move : move_list) {
						int move_label = make_move_label(Move(move), pos.side_to_move());
						float probability = y1[sample_index][move_label];
						sync_cout << move << " " << probability << sync_endl;
					}
					// 対局をやり直す。
				}
				else if (!pos.pos_is_ok()) {
					// 何らかの理由で局面が壊れた。
					sync_cout << "info string Position became illegal."
						<< " sample_index=" << sample_index << std::endl
						<< pos << sync_endl;
					// 対局をやり直す。
				}
				else {
					// 生成した局面を保存する。
					PackedSfenValue record = {};
					pos.sfen_pack(record.sfen);
					record.gamePly = pos.game_ply();
					game_state.records.push_back(record);


					// 選ばれた指し手を局面に適用する。
					pos.do_move(selected_move, *game_state.state_info_ptr++);
					continue;
				}
			}


			int game_result = GameResultDraw;
			u8 entering_king = 0;
			if (pos.is_mated()) {
				// 負け
				// 詰まされた
				// records.back()は相手局面なので勝ち
				game_result = GameResultWin;
			}
			else if (pos.DeclarationWin() != Move::none()) {
				// 勝ち
				// 入玉勝利
				// records.back()は相手局面なので負け
				game_result = GameResultLose;
				entering_king = 1;
			}

			if (game_result != GameResultDraw) {
				for (auto rit = game_state.records.rbegin(); rit != game_state.records.rend(); ++rit) {
					rit->game_result = game_result;
					rit->entering_king = entering_king;
					game_result = -game_result;
				}

				if (!game_state.records.empty()) {
					game_state.records.back().last_position = true;
				}

				for (const auto& record : game_state.records) {
					if (!kifu_writer->Write(record)) {
						sync_cout << "info string Failed to write a record." << sync_endl;
						std::exit(1);
					}
				}

				// 統計情報を更新する。
				++num_games;
				global_position_index += game_state.records.size();
			}

			// 次の対局の準備を始める。
			game_state.state_info_ptr = &game_state.state_info[0];
			game_state.records.clear();
			start_position_picker->Pick(pos, game_state.state_info_ptr, *Threads.main());
		}

		progress_report.Show(global_position_index);
	}

	std::cout << "num_games=" << num_games << " sum_plays=" << sum_plays << " sum_plays/num_games=" << sum_plays / num_games << std::endl;
	std::cout << "Finished." << std::endl;
}
