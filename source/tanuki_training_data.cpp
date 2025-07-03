#include "tanuki_training_data.h"

#include <filesystem>
#include <random>

#include "engine/dlshogi-engine/UctSearch.h"
#include "eval/deep/nn.h"
#include "eval/deep/nn_types.h"
#include "engine/dlshogi-engine/dlshogi_searcher.h"
#include "position.h"
#include "thread.h"

namespace
{
	// source\learn\learn.hよりコピー
	// PackedSfenと評価値が一体化した構造体
	// オプションごとに書き出す内容が異なると教師棋譜を再利用するときに困るので
	// とりあえず、以下のメンバーはオプションによらずすべて書き出しておく。
	struct PackedSfenValue
	{
		// 局面
		PackedSfen sfen;

		// Learner::search()から返ってきた評価値
		s16 score;

		// PVの初手
		// 教師との指し手一致率を求めるときなどに用いる
		u16 move;

		// 初期局面からの局面の手数。
		u16 gamePly;

		// この局面の手番側が、ゲームを最終的に勝っているなら1。負けているなら-1。
		// 引き分けに至った場合は、0。
		// 引き分けは、教師局面生成コマンドgensfenにおいて、
		// LEARN_GENSFEN_DRAW_RESULTが有効なときにだけ書き出す。
		s8 game_result;

		// 教師局面を書き出したファイルを他の人とやりとりするときに
		// この構造体サイズが不定だと困るため、paddingしてどの環境でも必ず40bytesになるようにしておく。
		u8 padding;

		// 32 + 2 + 2 + 2 + 1 + 1 = 40bytes

		bool operator<(const PackedSfenValue& rh) const {
			return std::memcmp(sfen.data, rh.sfen.data, sizeof(sfen.data)) < 0;
		}
	};

	static constexpr size_t BUFFER_SIZE = 1024 * 1024 * 1024;
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

extern DlshogiSearcher searcher;

void Tanuki::Rescore(std::istringstream& is)
{
	int batch_size = 1024;

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
	std::setvbuf(input_file, nullptr, _IOFBF, BUFFER_SIZE);

	FILE* output_file = std::fopen(output_file_path.c_str(), "wb");
	std::setvbuf(output_file, nullptr, _IOFBF, BUFFER_SIZE);

	int64_t num_processed = 0;
	int64_t progress_duration = 10000000;
	int64_t next_progress = progress_duration;
	std::vector<PackedSfenValue> packed_sfens(batch_size);
	while (!std::feof(input_file)) {
		size_t num_samples = std::fread(&packed_sfens[0], sizeof(PackedSfenValue), batch_size, input_file);

		for (int sample_index = 0; sample_index < num_samples; ++sample_index) {
			Position& position = Threads.main()->rootPos;
			StateInfo state_info;
			position.set_from_packed_sfen(packed_sfens[sample_index].sfen, &state_info, Threads.main());
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
		if (next_progress < num_processed) {
			std::cout << num_processed << std::endl;
			next_progress += progress_duration;
		}
	}

	std::cout << "Finished." << std::endl;

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
	for (const auto& input_file_path : input_file_paths) {
		input_files.push_back(std::fopen(input_file_path.c_str(), "rb"));
		std::setvbuf(input_files.back(), nullptr, _IOFBF, BUFFER_SIZE);
	}

	FILE* output_file = std::fopen(output_file_path.c_str(), "wb");
	std::setvbuf(output_file, nullptr, _IOFBF, BUFFER_SIZE);

	int64_t num_processed = 0;
	int64_t progress_duration = 10000000;
	int64_t next_progress = progress_duration;
	int num_input_files = static_cast<int>(input_file_paths.size());
	std::vector<std::vector<PackedSfenValue>> input_packed_sfens(
		num_input_files, std::vector<PackedSfenValue>(batch_size));
	while (!std::feof(input_files[0])) {
		size_t min_num_samples = std::numeric_limits<size_t>::max();
		for (int input_file_index = 0; input_file_index < static_cast<int>(num_input_files);
			++input_file_index) {
			size_t num_samples = std::fread(
				&input_packed_sfens[input_file_index][0], sizeof(PackedSfenValue), batch_size,
				input_files[input_file_index]);
			min_num_samples = std::min(min_num_samples, num_samples);
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
		if (next_progress < num_processed) {
			std::cout << num_processed << std::endl;
			next_progress += progress_duration;
		}
	}

	std::cout << "Finished." << std::endl;

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

	sync_cout << "info string " << (num_duplicated * 100.0 / num_records) << "% duplicated."  << sync_endl;

	std::fclose(output_file);
	output_file = nullptr;

	for (const auto& temp_file_path : temp_file_paths) {
		std::filesystem::remove(temp_file_path);
	}
}
