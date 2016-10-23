#ifndef _LEARNER_H_
#define _LEARNER_H_

#include <sstream>

#include "position.h"

namespace Learner
{
  struct Record {
    PackedSfen packed;
    int16_t value;
    Move pv[7];
  };
  static_assert(sizeof(Record) == 64, "Size of Record is not 64");

  constexpr char* OPTION_GENERATOR_NUM_GAMES = "GeneratorNumGames";
  constexpr char* OPTION_GENERATOR_MIN_SEARCH_DEPTH = "GeneratorMinSearchDepth";
  constexpr char* OPTION_GENERATOR_MAX_SEARCH_DEPTH = "GeneratorMaxSearchDepth";
  constexpr char* OPTION_GENERATOR_KIFU_TAG = "GeneratorKifuTag";
  constexpr char* OPTION_GENERATOR_BOOK_FILE_NAME = "GeneratorStartposFileName";
  constexpr char* OPTION_GENERATOR_MIN_BOOK_MOVE = "GeneratorMinBookMove";
  constexpr char* OPTION_GENERATOR_MAX_BOOK_MOVE = "GeneratorMaxBookMove";
  constexpr char* OPTION_LEARNER_NUM_POSITIONS = "LearnerNumPositions";
  constexpr char* OPTION_VALUE_HISTOGRAM_OUTPUT_FILE_PATH = "ValueHistogramOutputFilePath";
  constexpr char* OPTION_APPEARANCE_FREQUENCY_HISTOGRAM_OUTPUT_FILE_PATH = "AppearanceFrequencyHistogramOutputFilePath";

  void ShowProgress(
    const std::chrono::time_point<std::chrono::system_clock>& start, int64_t current_data,
    int64_t total_data, int64_t show_per);
  void Learn(std::istringstream& iss);
  void MeasureError();
  void BenchmarkKifuReader();
  void MeasureFillingFactor();
  void CalculateValueHistogram();
  void CalculateAppearanceFrequencyHistogram();
}

#endif