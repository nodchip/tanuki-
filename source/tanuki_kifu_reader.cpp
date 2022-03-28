#include "tanuki_kifu_reader.h"
#include "config.h"

#ifdef EVAL_LEARN

#include <filesystem>
#include <numeric>
#include <sstream>

#include "misc.h"
#include "usi.h"

using USI::Option;
using USI::OptionsMap;
using Learner::PackedSfenValue;

namespace {
	static const constexpr char* kKifuReaderBufferSize = "KifuReaderBufferSize";
}

Tanuki::KifuReader::KifuReader(const std::string& folder_name, int num_loops)
	: folder_name_(folder_name), num_loops_(num_loops) {}

Tanuki::KifuReader::~KifuReader() { Close(); }

bool Tanuki::KifuReader::Read(int num_records, std::vector<PackedSfenValue>& records) {
	records.resize(num_records);
	for (auto& record : records) {
		if (!Read(record)) {
			return false;
		}
	}
	return true;
}

bool Tanuki::KifuReader::Read(PackedSfenValue& record) {
	// ファイルリストを取得し、ファイルを開いた状態にする
	if (!EnsureOpen()) {
		return false;
	}

	// ループ終了条件は以下の通りとする
	if (file_index_ == static_cast<int>(file_paths_.size()) && loop_ == num_loops_) {
		return false;
	}

	// 1局面読み込む
	if (std::fread(&record, sizeof(record), 1, file_) == 1) {
		return true;
	}

	++file_index_;
	if (file_index_ == static_cast<int>(file_paths_.size())) {
		++loop_;
		if (loop_ == num_loops_) {
			return false;
		}

		file_index_ = 0;
	}

	if (std::fclose(file_)) {
		sync_cout << "info string Failed to close a kifu file." << sync_endl;
		return false;
	}

	file_ = std::fopen(file_paths_[file_index_].c_str(), "rb");
	if (file_ == nullptr) {
		// ファイルを開くのに失敗したら
		// 読み込みを終了する
		sync_cout << "into string Failed to open a kifu file: " << file_paths_[file_index_]
			<< sync_endl;
		return false;
	}

	int buffer_size = Options[kKifuReaderBufferSize];
	if (std::setvbuf(file_, nullptr, _IOFBF, buffer_size)) {
		sync_cout << "into string Failed to set a file buffer: " << file_paths_[file_index_]
			<< sync_endl;
	}

	if (std::fread(&record, sizeof(record), 1, file_) != 1) {
		return false;
	}

	return true;
}

bool Tanuki::KifuReader::Close() {
	if (fclose(file_)) {
		return false;
	}
	file_ = nullptr;
	return true;
}

bool Tanuki::KifuReader::EnsureOpen() {
	if (!file_paths_.empty()) {
		return true;
	}

	for (const auto& entry : std::filesystem::directory_iterator(folder_name_)) {
		if (!entry.is_regular_file()) {
			continue;
		}
		file_paths_.push_back(entry.path().string());
	}

	if (file_paths_.empty()) {
		return false;
	}

	file_ = std::fopen(file_paths_[0].c_str(), "rb");

	if (!file_) {
		sync_cout << "into string Failed to open a kifu file: " << file_paths_[0] << sync_endl;
		return false;
	}

	int buffer_size = Options[kKifuReaderBufferSize];
	if (std::setvbuf(file_, nullptr, _IOFBF, buffer_size)) {
		sync_cout << "into string Failed to set a file buffer: " << file_paths_[0] << sync_endl;
	}

	return true;
}

void Tanuki::KifuReader::Initialize(USI::OptionsMap& o) {
	o[kKifuReaderBufferSize] << USI::Option(1024 * 1024, 0, std::numeric_limits<int>::max());
}

#endif
