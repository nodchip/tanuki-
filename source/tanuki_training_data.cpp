#include "tanuki_training_data.h"

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

extern DlshogiSearcher searcher;

void Tanuki::Rescore()
{
	int batch_size = 1024;

	Options["DNN_Batch_Size1"] = std::to_string(batch_size);

	is_ready();
	
	std::string input_file_path = R"(D:\hnoda\shogi\training_data\tanuki-.nnue-pytorch-2024-07-30.1.shuffled\shuffled.bin)";
	std::string output_file_path = R"(D:\hnoda\shogi\training_data\tanuki-.nnue-pytorch-2024-07-30.1.shuffled\shuffled.20240329_153000_model_resnet30x384_relu_027.bin)";
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
	std::setvbuf(input_file, nullptr, _IOFBF, 1024 * 1024 * 1024);

	FILE* output_file = std::fopen(output_file_path.c_str(), "wb");
	std::setvbuf(output_file, nullptr, _IOFBF, 1024 * 1024 * 1024);

	int64_t num_processed = 0;
	int64_t progress_duration = 1000000;
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
