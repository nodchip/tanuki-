#ifndef CSA_HPP
#define CSA_HPP

#include <filesystem>
#include <string>
#include <vector>

#include "position.hpp"
#include "game_record.hpp"

namespace csa {
  extern const std::filesystem::path DEFAULT_INPUT_CSA1_FILE_PATH;
  extern const std::filesystem::path DEFAULT_OUTPUT_SFEN_FILE_PATH;

  // CSAファイルをsfen形式へ変換する
  bool toSfen(const std::filesystem::path& filepath, std::vector<std::string>& sfen);

  // CSAファイルが勝負が終わっているかどうかを返す
  bool isFinished(const std::filesystem::path& filepath);

  // CSAファイル中でtanuki-が先手かどうかを返す
  bool isTanukiBlack(const std::filesystem::path& filepath);

  // CSAファイル中でどちらが勝ったかを返す
  // 引き分けの場合はColorNumを返す
  Color getWinner(const std::filesystem::path& filepath);

  // floodgateのCSAファイルをSFEN形式へ変換する
  bool convertCsaToSfen(
    const std::filesystem::path& inputDirectoryPath,
    const std::filesystem::path& outputFilePath);

  // 2chkifu.csa1をSFEN形式へ変換する
  bool convertCsa1LineToSfen(
    const std::filesystem::path& inputFilePath = DEFAULT_INPUT_CSA1_FILE_PATH,
    const std::filesystem::path& outputFilePath = DEFAULT_OUTPUT_SFEN_FILE_PATH);

  // CSAファイルを読み込む
  bool readCsa(const std::filesystem::path& filepath, GameRecord& gameRecord);

  // サブディレクトリも含めてCSAファイルを読み込む
  // filterがtrueとなるファイルのみ処理する
  bool readCsas(
    const std::filesystem::path& directory,
    const std::function<bool(const std::filesystem::path&)>& pathFilter,
    const std::function<bool(const GameRecord&)>& gameRecordFilter,
    std::vector<GameRecord>& gameRecords);
}

#endif
