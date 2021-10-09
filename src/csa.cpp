#include <filesystem>
#include <fstream>
#include "timeManager.hpp"
#include "csa.hpp"
#include "search.hpp"
#include "string_util.hpp"
#include "time_util.hpp"
#include "usi.hpp"

using namespace std::filesystem;

namespace
{
  int getNumberOfFiles(const std::filesystem::path& directory)
  {
    int numberOfFiles = 0;
    for (std::filesystem::recursive_directory_iterator it(directory);
    it != std::filesystem::recursive_directory_iterator();
      ++it)
    {
      if (++numberOfFiles % 100000 == 0) {
        std::cout << numberOfFiles << std::endl;
      }
    }
    return numberOfFiles;
  }
}

const std::filesystem::path csa::DEFAULT_INPUT_CSA1_FILE_PATH = "../2chkifu_csa/2chkifu.csa1";
const std::filesystem::path csa::DEFAULT_OUTPUT_SFEN_FILE_PATH = "../bin/kifu.sfen";

bool csa::toSfen(const std::filesystem::path& filepath, std::vector<std::string>& sfen) {
  sfen.clear();
  sfen.push_back("startpos");
  sfen.push_back("moves");

  Position position;
  USI::setPosition(position, string_util::concat(sfen));

  std::ifstream ifs(filepath);
  if (!ifs.is_open()) {
    std::cout << "!!! Failed to open a CSA file." << std::endl;
    return false;
  }

  // Position::doMove()は前回と違うアドレスに確保されたStateInfoを要求するため
  // listを使って過去のStateInfoを保持する。
  std::list<StateInfo> stateInfos;

  std::string line;
  while (std::getline(ifs, line)) {
    // 将棋所の出力するCSAの指し手の末尾に",T1"などとつくため
    // ","以降を削除する
    if (line.find(',') != std::string::npos) {
      line = line.substr(0, line.find(','));
    }

    if (line.size() != 7 || (line[0] != '+' && line[0] != '-')) {
      continue;
    }

    std::string csaMove = line.substr(1);
    Move move = USI::csaToMove(position, csaMove);

#if !defined NDEBUG
    if (!position.moveIsLegal(move)) {
      std::cout << "!!! Found an illegal move." << std::endl;
      break;
    }
#endif

    stateInfos.push_back(StateInfo());
    position.doMove(move, stateInfos.back());
    //position.print();

    sfen.push_back(move.toUSI());
  }

  return true;
}

bool csa::isFinished(const std::filesystem::path& filepath) {
  std::ifstream ifs(filepath);
  if (!ifs.is_open()) {
    std::cout << "!!! Failed to open a CSA file." << std::endl;
    return false;
  }

  std::string line;
  while (std::getline(ifs, line)) {
    if (line.find("%TORYO") == 0) {
      return true;
    }
  }

  return false;
}

bool csa::isTanukiBlack(const std::filesystem::path& filepath) {
  std::ifstream ifs(filepath);
  if (!ifs.is_open()) {
    std::cout << "!!! Failed to open a CSA file." << std::endl;
    return false;
  }

  std::string line;
  while (std::getline(ifs, line)) {
    if (line.find("N+tanuki-") == 0) {
      return true;
    }
  }

  return false;
}

Color csa::getWinner(const std::filesystem::path& filepath) {
  assert(isFinished(filepath));

  std::ifstream ifs(filepath);
  if (!ifs.is_open()) {
    std::cout << "!!! Failed to open a CSA file." << std::endl;
    return ColorNum;
  }

  char turn = 0;
  std::string line;
  while (std::getline(ifs, line)) {
    if (line.empty()) {
      continue;
    }
    if (line[0] == '+' || line[0] == '-') {
      turn = line[0];
    }
    if (line.find("%TORYO") == 0) {
      return turn == '+' ? Black : White;
    }
  }

  return ColorNum;
}

// 文字列の配列をスペース区切りで結合する
static void concat(const std::vector<std::string>& words, std::string& out) {
  out.clear();
  for (const auto& word : words) {
    if (!out.empty()) {
      out += " ";
    }
    out += word;
  }
}

bool csa::convertCsaToSfen(
  const std::filesystem::path& inputDirectoryPath,
  const std::filesystem::path& outputFilePath) {
  if (!is_directory(inputDirectoryPath)) {
    std::cout << "!!! Failed to open the input directory: inputDirectoryPath="
      << inputDirectoryPath
      << std::endl;
    return false;
  }

  std::ofstream ofs(outputFilePath, std::ios::out);
  if (!ofs.is_open()) {
    std::cout << "!!! Failed to create the output file: outputTeacherFilePath="
      << outputFilePath
      << std::endl;
    return false;
  }

  int numberOfFiles = std::distance(
    directory_iterator(inputDirectoryPath),
    directory_iterator());
  int fileIndex = 0;
  for (auto it = directory_iterator(inputDirectoryPath); it != directory_iterator(); ++it) {
    if (++fileIndex % 1000 == 0) {
      printf("(%d/%d)\n", fileIndex, numberOfFiles);
    }

    const auto& inputFilePath = *it;
    std::vector<std::string> sfen;
    if (!toSfen(inputFilePath, sfen)) {
      std::cout << "!!! Failed to convert the input csa file to SFEN: inputFilePath="
        << inputFilePath.path()
        << std::endl;
      continue;
    }

    std::string line;
    concat(sfen, line);
    ofs << line << std::endl;
  }

  return true;
}

bool csa::convertCsa1LineToSfen(
  const std::filesystem::path& inputFilePath,
  const std::filesystem::path& outputFilePath) {
  std::ifstream ifs(inputFilePath);
  if (!ifs.is_open()) {
    std::cout << "!!! Failed to open the input file: inputFilePath="
      << inputFilePath
      << std::endl;
    return false;
  }

  std::ofstream ofs(outputFilePath, std::ios::out);
  if (!ofs.is_open()) {
    std::cout << "!!! Failed to create the output file: outputTeacherFilePath="
      << outputFilePath
      << std::endl;
    return false;
  }

  std::string line;
  while (std::getline(ifs, line)) {
    int kifuIndex;
    std::stringstream ss(line);
    ss >> kifuIndex;

    // 進捗状況表示
    if (kifuIndex % 1000 == 0) {
      std::cout << kifuIndex << std::endl;
    }

    std::string elem;
    ss >> elem; // 対局日を飛ばす。
    ss >> elem; // 先手
    const std::string sente = elem;
    ss >> elem; // 後手
    const std::string gote = elem;
    ss >> elem; // (0:引き分け,1:先手の勝ち,2:後手の勝ち)

    if (!std::getline(ifs, line)) {
      std::cout << "!!! header only !!!" << std::endl;
      return false;
    }

    ofs << "startpos moves";

    Position pos;
    pos.set(USI::DefaultStartPositionSFEN, Threads.main());
    Search::StateStackPtr SetUpStates = Search::StateStackPtr(new std::stack<StateInfo>());
    while (!line.empty()) {
      const std::string moveStrCSA = line.substr(0, 6);
      const Move move = USI::csaToMove(pos, moveStrCSA);
      if (move.isNone()) {
        pos.print();
        std::cout << "!!! Illegal move = " << moveStrCSA << " !!!" << std::endl;
        break;
      }
      line.erase(0, 6); // 先頭から6文字削除

      ofs << " " << move.toUSI();

      SetUpStates->push(StateInfo());
      pos.doMove(move, SetUpStates->top());
    }

    ofs << std::endl;
  }

  return true;
}

bool csa::readCsa(const std::filesystem::path& filepath, GameRecord& gameRecord)
{
  gameRecord.gameRecordIndex = 0;
  gameRecord.date = "??/??/??";
  gameRecord.winner = 0;
  gameRecord.leagueName = "???";
  gameRecord.strategy = "???";

  Position position;
  StateInfo state_info[512];
  USI::setPosition(position, "startpos moves");

  std::ifstream ifs(filepath);
  if (!ifs.is_open()) {
    std::cout << "!!! Failed to open the input file: filepath="
      << filepath
      << std::endl;
    return false;
  }

  // Position::doMove()は前回と違うアドレスに確保されたStateInfoを要求するため
  // listを使って過去のStateInfoを保持する。
  std::list<StateInfo> stateInfos;

  std::string line;
  bool toryo = false;
  while (std::getline(ifs, line)) {
    // 将棋所の出力するCSAの指し手の末尾に",T1"などとつくため
    // ","以降を削除する
    if (line.find(',') != std::string::npos) {
      line = line.substr(0, line.find(','));
    }

    if (line.find("N+") == 0) {
      gameRecord.blackPlayerName = line.substr(2);
    }
    else if (line.find("N-") == 0) {
      gameRecord.whitePlayerName = line.substr(2);
    }
    else if (line.size() == 7 && (line[0] == '+' || line[0] == '-')) {
      std::string csaMove = line.substr(1);
      Move move = USI::csaToMove(position, csaMove);

      if (!position.moveIsPseudoLegal(move)) {
        std::cout << "!!! Found an illegal move." << std::endl;
        return false;
      }

      stateInfos.push_back(StateInfo());
      position.doMove(move, stateInfos.back());
      //position.print();

      gameRecord.moves.push_back(move);
    }
    else if (line.find(gameRecord.blackPlayerName + " win") != std::string::npos) {
      gameRecord.winner = 1;
    }
    else if (line.find(gameRecord.whitePlayerName + " win") != std::string::npos) {
      gameRecord.winner = 2;
    }
    else if (line.find("toryo") != std::string::npos) {
      toryo = true;
    }
  }

  // 投了以外の棋譜はスキップする
  if (!toryo) {
    gameRecord.numberOfPlays = -1;
  }

  gameRecord.numberOfPlays = gameRecord.moves.size();
  return true;
}

bool csa::readCsas(
  const std::filesystem::path& directory,
  const std::function<bool(const std::filesystem::path&)>& pathFilter,
  const std::function<bool(const GameRecord&)>& gameRecordFilter,
  std::vector<GameRecord>& gameRecords)
{
  std::cout << "Listing files ..." << std::endl;
  int numberOfFiles = getNumberOfFiles(directory);

  double startClockSec = clock() / double(CLOCKS_PER_SEC);
  int fileIndex = 0;
  for (std::filesystem::recursive_directory_iterator it(directory);
  it != std::filesystem::recursive_directory_iterator();
    ++it)
  {
    if (++fileIndex % 10000 == 0) {
      std::cout << time_util::formatRemainingTime(
        startClockSec, fileIndex, numberOfFiles);
    }

    if (!pathFilter(it->path())) {
      continue;
    }

    GameRecord gameRecord;
    if (!readCsa(it->path(), gameRecord)) {
      std::cout << "Skipped" << std::endl;
      continue;
    }

    if (!gameRecordFilter(gameRecord)) {
      continue;
    }

    gameRecords.push_back(gameRecord);
  }

  return true;
}
