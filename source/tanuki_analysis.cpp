#include "tanuki_analysis.h"
#include "config.h"

#include "tanuki_kifu_reader.h"
#include "tanuki_progress.h"
#include "thread.h"
#include "usi.h"

#ifdef EVAL_LEARN

namespace
{
	const int kNumBars = 100;
	const int kNumPositions = 10000000;
}

void Tanuki::AnalyzeProgress()
{
	auto kifu_folder_path = Options["KifuDir"];
	KifuReader reader(kifu_folder_path, 1);

	Progress progress;
	if (!progress.Load()) {
		std::exit(1);
	}

	int count[kNumBars] = {};
	int64_t sum_abs_value[kNumBars] = {};

	Learner::PackedSfenValue packed_sfen_value;
	auto& position = Threads[0]->rootPos;
	int num_positions = 0;
	for (; num_positions < kNumPositions && reader.Read(packed_sfen_value); ++num_positions) {
		StateInfo state_info = {};
		position.set_from_packed_sfen(packed_sfen_value.sfen, &state_info, Threads[0]);
		double p = progress.Estimate(position);
		++count[static_cast<int>(p * kNumBars)];
		++num_positions;
		sum_abs_value[static_cast<int>(p * kNumBars)] += std::abs(packed_sfen_value.score);
	}

	double num_positions_adjusted = 0.0;
	for (int i = 0; i < kNumBars; ++i) {
		num_positions_adjusted += count[i] * (kNumBars - i) / static_cast<double>(kNumBars);
	}

	for (int i = 0; i < kNumBars; ++i) {
		printf("%f,%f,%f\n",
			i / static_cast<double>(kNumBars),
			count[i] / static_cast<double>(num_positions),
			count[i] ? sum_abs_value[i] / static_cast<double>(count[i]) : 0.0);
	}
}

#endif
