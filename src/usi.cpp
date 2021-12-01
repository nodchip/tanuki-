#include <memory>
#include "timeManager.hpp"
#include "benchmark.hpp"
#include "book.hpp"
#include "generateMoves.hpp"
#include "learner.hpp"
#include "move.hpp"
#include "movePicker.hpp"
#include "position.hpp"
#include "search.hpp"
#include "thread.hpp"
#include "tt.hpp"
#include "usi.hpp"
#include "csa1.hpp"
#include "thread.hpp"

#ifdef _MSC_VER
#include "csa.hpp"
#endif

USI::OptionsMap Options;

namespace {
  void onThreads(const USI::USIOption&) { Threads.read_usi_options(); }
  void onHashSize(const USI::USIOption& opt) { TT.resize(opt); }
  void onClearHash(const USI::USIOption&) { Search::clear(); }
  void onEvalDir(const USI::USIOption& opt) {
    std::unique_ptr<Evaluater>(new Evaluater)->init(opt, true);
  }
}

bool USI::CaseInsensitiveLess::operator () (const std::string& s1, const std::string& s2) const {
  for (size_t i = 0; i < s1.size() && i < s2.size(); ++i) {
    const int c1 = tolower(s1[i]);
    const int c2 = tolower(s2[i]);

    if (c1 != c2) {
      return c1 < c2;
    }
  }
  return s1.size() < s2.size();
}

namespace {
  // 論理的なコア数の取得
  inline int cpuCoreCount() {
    // todo: boost::thread::physical_concurrency() を使うこと。
    // std::thread::hardware_concurrency() は 0 を返す可能性がある。
    return std::max(static_cast<int>(std::thread::hardware_concurrency()), 1);
  }

  class StringToPieceTypeCSA : public std::map<std::string, PieceType> {
  public:
    StringToPieceTypeCSA() {
      (*this)["FU"] = Pawn;
      (*this)["KY"] = Lance;
      (*this)["KE"] = Knight;
      (*this)["GI"] = Silver;
      (*this)["KA"] = Bishop;
      (*this)["HI"] = Rook;
      (*this)["KI"] = Gold;
      (*this)["OU"] = King;
      (*this)["TO"] = ProPawn;
      (*this)["NY"] = ProLance;
      (*this)["NK"] = ProKnight;
      (*this)["NG"] = ProSilver;
      (*this)["UM"] = Horse;
      (*this)["RY"] = Dragon;
    }
    PieceType value(const std::string& str) const {
      return this->find(str)->second;
    }
    bool isLegalString(const std::string& str) const {
      return (this->find(str) != this->end());
    }
  };
  const StringToPieceTypeCSA g_stringToPieceTypeCSA;
}

const USI::USIOption USI::OptionsMap::INVALID_OPTION;

void USI::OptionsMap::init() {
  (*this)[USI::OptionNames::USI_HASH] = USIOption(256, 1, 65536, onHashSize);
  (*this)[USI::OptionNames::CLEAR_HASH] = USIOption(onClearHash);
  (*this)[USI::OptionNames::BOOK_FILE] = USIOption("../bin/book-2016-02-01.bin");
  (*this)[USI::OptionNames::BEST_BOOK_MOVE] = USIOption(false);
  (*this)[USI::OptionNames::OWNBOOK] = USIOption(true);
  (*this)[USI::OptionNames::MIN_BOOK_PLY] = USIOption(SHRT_MAX, 0, SHRT_MAX);
  (*this)[USI::OptionNames::MAX_BOOK_PLY] = USIOption(SHRT_MAX, 0, SHRT_MAX);
  (*this)[USI::OptionNames::MIN_BOOK_SCORE] = USIOption(-180, -ScoreInfinite, ScoreInfinite);
  (*this)[USI::OptionNames::EVAL_DIR] = USIOption("../bin/20151105", onEvalDir);
  (*this)[USI::OptionNames::WRITE_SYNTHESIZED_EVAL] = USIOption(false);
  (*this)[USI::OptionNames::USI_PONDER] = USIOption(true);
  (*this)[USI::OptionNames::BYOYOMI_MARGIN] = USIOption(500, 0, INT_MAX);
  (*this)[USI::OptionNames::MULTIPV] = USIOption(1, 1, MaxLegalMoves);
  (*this)[USI::OptionNames::SKILL_LEVEL] = USIOption(20, 0, 20);
  (*this)[USI::OptionNames::MAX_RANDOM_SCORE_DIFF] = USIOption(0, 0, ScoreMate0Ply);
  (*this)[USI::OptionNames::MAX_RANDOM_SCORE_DIFF_PLY] = USIOption(0, 0, SHRT_MAX);
  (*this)[USI::OptionNames::SLOW_MOVER] = USIOption(50, 10, 1000);
  (*this)[USI::OptionNames::MINIMUM_THINKING_TIME] = USIOption(1500, 0, INT_MAX);
  (*this)[USI::OptionNames::MAX_THREADS_PER_SPLIT_POINT] = USIOption(5, 4, 8, onThreads);
  (*this)[USI::OptionNames::THREADS] = USIOption(cpuCoreCount(), 1, MaxThreads, onThreads);
  (*this)[USI::OptionNames::USE_SLEEPING_THREADS] = USIOption(true);
  (*this)[USI::OptionNames::OUTPUT_INFO] = USIOption(true);
  (*this)[USI::OptionNames::SEARCH_WINDOW_OFFSET] = USIOption(0, -1024, 1024);
  (*this)[USI::OptionNames::MOVE_OVERHEAD] = USIOption(30, 0, 5000);
  (*this)[USI::OptionNames::NODESTIME] = USIOption(0, 0, 10000);
  (*this)[USI::OptionNames::BOOK_SLEEP_TIME] = USIOption(0, 0, INT_MAX);
#if defined BISHOP_IN_DANGER
  (*this)[USI::OptionNames::DANGER_DEMERIT_SCORE] = USIOption(700, SHRT_MIN, SHRT_MAX);
#endif
}

USI::USIOption::USIOption(const char* v, Fn* f) :
  type_("string"), min_(0), max_(0), onChange_(f)
{
  defaultValue_ = currentValue_ = v;
}

USI::USIOption::USIOption(const bool v, Fn* f) :
  type_("check"), min_(0), max_(0), onChange_(f)
{
  defaultValue_ = currentValue_ = (v ? "true" : "false");
}

USI::USIOption::USIOption(Fn* f) :
  type_("button"), min_(0), max_(0), onChange_(f) {}

USI::USIOption::USIOption(const int v, const int min, const int max, Fn* f)
  : type_("spin"), min_(min), max_(max), onChange_(f)
{
  std::ostringstream ss;
  ss << v;
  defaultValue_ = currentValue_ = ss.str();
}

USI::USIOption& USI::USIOption::operator = (const std::string& v) {
  assert(!type_.empty());

  if ((type_ != "button" && v.empty())
    || (type_ == "check" && v != "true" && v != "false")
    || (type_ == "spin" && (atoi(v.c_str()) < min_ || max_ < atoi(v.c_str()))))
  {
    return *this;
  }

  if (type_ != "button") {
    currentValue_ = v;
  }

  if (onChange_ != nullptr) {
    (*onChange_)(*this);
  }

  return *this;
}

std::ostream& operator << (std::ostream& os, const USI::OptionsMap& om) {
  for (auto& elem : om) {
    const USI::USIOption& o = elem.second;
    os << "\noption name " << elem.first << " type " << o.type();
    if (o.type() != "button") {
      os << " default " << o.defaultValue();
    }

    if (o.type() == "spin") {
      os << " min " << o.min() << " max " << o.max();
    }
  }
  return os;
}

void USI::go(const Position& pos, const std::string& cmd) {
  std::istringstream iss(cmd);
  go(pos, iss);
}

void USI::go(const Position& pos, std::istringstream& ssCmd) {
  Search::LimitsType limits;
  limits.startTime = now(); // As early as possible!
  std::vector<Move> moves;
  std::string token;

  while (ssCmd >> token) {
    if (token == "ponder") { limits.ponder = true; }
    else if (token == "btime") {
      int btime;
      ssCmd >> btime;
      limits.time[Black] = btime;
    }
    else if (token == "wtime") {
      int wtime;
      ssCmd >> wtime;
      limits.time[White] = wtime;
    }
    else if (token == "winc") {
      int winc;
      ssCmd >> winc;
      winc = std::max(0, winc - Options[USI::OptionNames::BYOYOMI_MARGIN]);
      limits.inc[White] = winc;
    }
    else if (token == "binc") {
      int binc;
      ssCmd >> binc;
      binc = std::max(0, binc - Options[USI::OptionNames::BYOYOMI_MARGIN]);
      limits.inc[Black] = binc;
    }
    else if (token == "infinite") { limits.infinite = true; }
    else if (token == "byoyomi" || token == "movetime") {
      // btime wtime の後に byoyomi が来る前提になっているので良くない。
      int byoyomi;
      ssCmd >> byoyomi;
      byoyomi = std::max(0, byoyomi - Options[USI::OptionNames::BYOYOMI_MARGIN]);
      limits.byoyomi = byoyomi;
    }
    else if (token == "depth") {
      int depth;
      ssCmd >> depth;
      limits.depth = depth;
    }
    else if (token == "nodes") {
      int nodes;
      ssCmd >> nodes;
      limits.nodes = nodes;
    }
    else if (token == "searchmoves") {
      while (ssCmd >> token)
        moves.push_back(usiToMove(pos, token));
    }
  }
  limits.searchmoves = moves;
  Search::BroadcastPvDepth = 0;
  Threads.start_thinking(pos, limits, Search::SetupStates);
}

#if defined LEARN
// 学習用。通常の go 呼び出しは文字列を扱って高コストなので、大量に探索の開始、終了を行う学習では別の呼び出し方にする。
void go(const Position& pos, const Ply depth, const Move move) {
  LimitsType limits;
  std::vector<Move> moves;
  limits.depth = depth;
  moves.push_back(move);
  pos.searcher()->threads.startThinking(pos, limits, moves, std::chrono::system_clock::now());
}
#endif

Move usiToMoveBody(const Position& pos, const std::string& moveStr) {
  Move move;
  if (g_charToPieceUSI.isLegalChar(moveStr[0])) {
    // drop
    const PieceType ptTo = pieceToPieceType(g_charToPieceUSI.value(moveStr[0]));
    if (moveStr[1] != '*') {
      return Move::moveNone();
    }
    const File toFile = charUSIToFile(moveStr[2]);
    const Rank toRank = charUSIToRank(moveStr[3]);
    if (!isInSquare(toFile, toRank)) {
      return Move::moveNone();
    }
    const Square to = makeSquare(toFile, toRank);
    move = makeDropMove(ptTo, to);
  }
  else {
    const File fromFile = charUSIToFile(moveStr[0]);
    const Rank fromRank = charUSIToRank(moveStr[1]);
    if (!isInSquare(fromFile, fromRank)) {
      return Move::moveNone();
    }
    const Square from = makeSquare(fromFile, fromRank);
    const File toFile = charUSIToFile(moveStr[2]);
    const Rank toRank = charUSIToRank(moveStr[3]);
    if (!isInSquare(toFile, toRank)) {
      return Move::moveNone();
    }
    const Square to = makeSquare(toFile, toRank);
    if (moveStr[4] == '\0') {
      move = makeNonPromoteMove<Capture>(pieceToPieceType(pos.piece(from)), from, to, pos);
    }
    else if (moveStr[4] == '+') {
      if (moveStr[5] != '\0') {
        return Move::moveNone();
      }
      move = makePromoteMove<Capture>(pieceToPieceType(pos.piece(from)), from, to, pos);
    }
    else {
      return Move::moveNone();
    }
  }

  if (pos.moveIsPseudoLegal(move, true)
    && pos.pseudoLegalMoveIsLegal<false, false, true>(move, pos.pinnedBB()))
  {
    return move;
  }
  return Move::moveNone();
}
#if !defined NDEBUG
// for debug
Move usiToMoveDebug(const Position& pos, const std::string& moveStr) {
  for (MoveList<LegalAll> ml(pos); !ml.end(); ++ml) {
    if (moveStr == ml.move().toUSI()) {
      return ml.move();
    }
  }
  return Move::moveNone();
}
Move csaToMoveDebug(const Position& pos, const std::string& moveStr) {
  for (MoveList<LegalAll> ml(pos); !ml.end(); ++ml) {
    if (moveStr == ml.move().toCSA()) {
      return ml.move();
    }
  }
  return Move::moveNone();
}
#endif
Move USI::usiToMove(const Position& pos, const std::string& moveStr) {
  const Move move = usiToMoveBody(pos, moveStr);
  assert(move == usiToMoveDebug(pos, moveStr));
  return move;
}

Move csaToMoveBody(const Position& pos, const std::string& moveStr) {
  if (moveStr.size() != 6) {
    return Move::moveNone();
  }
  const File toFile = charCSAToFile(moveStr[2]);
  const Rank toRank = charCSAToRank(moveStr[3]);
  if (!isInSquare(toFile, toRank)) {
    return Move::moveNone();
  }
  const Square to = makeSquare(toFile, toRank);
  const std::string ptToString(moveStr.begin() + 4, moveStr.end());
  if (!g_stringToPieceTypeCSA.isLegalString(ptToString)) {
    return Move::moveNone();
  }
  const PieceType ptTo = g_stringToPieceTypeCSA.value(ptToString);
  Move move;
  if (moveStr[0] == '0' && moveStr[1] == '0') {
    // drop
    move = makeDropMove(ptTo, to);
  }
  else {
    const File fromFile = charCSAToFile(moveStr[0]);
    const Rank fromRank = charCSAToRank(moveStr[1]);
    if (!isInSquare(fromFile, fromRank)) {
      return Move::moveNone();
    }
    const Square from = makeSquare(fromFile, fromRank);
    PieceType ptFrom = pieceToPieceType(pos.piece(from));
    if (ptFrom == ptTo) {
      // non promote
      move = makeNonPromoteMove<Capture>(ptFrom, from, to, pos);
    }
    else if (ptFrom + PTPromote == ptTo) {
      // promote
      move = makePromoteMove<Capture>(ptFrom, from, to, pos);
    }
    else {
      return Move::moveNone();
    }
  }

  if (pos.moveIsPseudoLegal(move, true)
    && pos.pseudoLegalMoveIsLegal<false, false, true>(move, pos.pinnedBB()))
  {
    return move;
  }
  return Move::moveNone();
}
Move USI::csaToMove(const Position& pos, const std::string& moveStr) {
  const Move move = csaToMoveBody(pos, moveStr);
  assert(move == csaToMoveDebug(pos, moveStr));
  return move;
}

void USI::setPosition(Position& pos, const std::string& cmd)
{
  std::istringstream iss(cmd);
  setPosition(pos, iss);
}

void USI::setPosition(Position& pos, std::istringstream& ssCmd) {
  std::string token;
  std::string sfen;

  ssCmd >> token;

  if (token == "startpos") {
    sfen = DefaultStartPositionSFEN;
    ssCmd >> token; // "moves" が入力されるはず。
  }
  else if (token == "sfen") {
    while (ssCmd >> token && token != "moves") {
      sfen += token + " ";
    }
  }
  else {
    return;
  }

  pos.set(sfen, Threads.main());
  Search::SetupStates = Search::StateStackPtr(new std::stack<StateInfo>());

  Ply currentPly = pos.gamePly();
  while (ssCmd >> token) {
    const Move move = usiToMove(pos, token);
    if (move.isNone()) break;
    Search::SetupStates->push(StateInfo());
    pos.doMove(move, Search::SetupStates->top());
    ++currentPly;
  }
  pos.setStartPosPly(currentPly);
}

void USI::setOption(const std::string& cmd)
{
  std::istringstream iss(cmd);
  setOption(iss);
}

void USI::setOption(std::istringstream& ssCmd) {
  std::string token;
  std::string name;
  std::string value;

  ssCmd >> token; // "name" が入力されるはず。

  ssCmd >> name;
  // " " が含まれた名前も扱う。
  while (ssCmd >> token && token != "value") {
    name += " " + token;
  }

  ssCmd >> value;
  // " " が含まれた値も扱う。
  while (ssCmd >> token) {
    value += " " + token;
  }

  if (!Options.isLegalOption(name)) {
    std::cout << "No such option: " << name << std::endl;
  }
  else {
    Options[name] = value;
  }
}

#ifdef NDEBUG
#ifdef MY_NAME
const std::string MyName = MY_NAME;
#else
const std::string MyName = "tanuki-";
#endif
#else
const std::string MyName = "tanuki- Debug Build";
#endif

void USI::doUSICommandLoop(int argc, char* argv[]) {
  Position pos(USI::DefaultStartPositionSFEN, Threads.main());

  std::string cmd;
  std::string token;
  std::string lastPositionCmd;

#if defined MPI_LEARN
  boost::mpi::environment  env(argc, argv);
  boost::mpi::communicator world;
  if (world.rank() != 0) {
    learn(pos, env, world);
    return;
  }
#endif

  for (int i = 1; i < argc; ++i)
    cmd += std::string(argv[i]) + " ";

  do {
    if (argc == 1)
      std::getline(std::cin, cmd);

    std::istringstream ssCmd(cmd);

    ssCmd >> std::skipws >> token;

    if (token == "quit" ||
      token == "stop" ||
      (token == "ponderhit" && Search::Signals.stopOnPonderhit) ||
      token == "gameover") {
      Search::Signals.stop = true;
      Threads.main()->start_searching(true);
    }
    else if (token == "ponderhit") {
      Search::Limits.ponder = 0;
    }
    else if (token == "usinewgame") {
      TT.clear();
      Search::book.open(((std::string)Options[USI::OptionNames::BOOK_FILE]).c_str());
#if defined INANIWA_SHIFT
      inaniwaFlag = NotInaniwa;
#endif
#if defined BISHOP_IN_DANGER
      bishopInDangerFlag = NotBishopInDanger;
#endif
      for (int i = 0; i < 100; ++i) g_randomTimeSeed(); // 最初は乱数に偏りがあるかも。少し回しておく。
    }
    else if (token == "usi") {
      SYNCCOUT << "id name " << MyName
        << "\nid author nodchip"
        << "\n" << Options
        << "\nusiok" << SYNCENDL;
    }
    else if (token == "go") {
      USI::go(pos, ssCmd);
      SYNCCOUT << "info string " << lastPositionCmd << SYNCENDL;
    }
    else if (token == "isready") { SYNCCOUT << "readyok" << SYNCENDL; }
    else if (token == "position") {
      lastPositionCmd = cmd;
      USI::setPosition(pos, ssCmd);
    }
    else if (token == "setoption") { USI::setOption(ssCmd); }
    else if (token == "broadcast") {
      std::getline(ssCmd, Search::BroadcastPvInfo);

      Search::BroadcastPvDepth = 0;
      std::istringstream iss(Search::BroadcastPvInfo);
      std::string term;
      while (iss >> term) {
        if (term != "depth") {
          continue;
        }
        int depth;
        iss >> depth;

        {
          std::lock_guard<std::mutex> lock(Search::BroadcastMutex);
          Search::BroadcastPvDepth = std::max(Search::BroadcastPvDepth, depth);
        }

        break;
      }
    }
#if defined LEARN
    else if (token == "l") {
      auto learner = std::unique_ptr<Learner>(new Learner());
#if defined MPI_LEARN
      learner->learn(pos, env, world);
#else
      learner->learn(pos, ssCmd);
#endif
    }
#endif
#if !defined MINIMUL
    // 以下、デバッグ用
    else if (token == "bench") { benchmark(pos); }
    else if (token == "benchmark_elapsed_for_depth_n") { benchmarkElapsedForDepthN(pos); }
    else if (token == "benchmark_search_window") { benchmarkSearchWindow(pos); }
    else if (token == "benchmark_generate_moves") { benchmarkGenerateMoves(pos); }
    else if (token == "d") { pos.print(); }
    else if (token == "t") { std::cout << pos.mateMoveIn1Ply().toCSA() << std::endl; }
    else if (token == "b") { makeBook(pos, ssCmd); }
#ifdef _MSC_VER
    else if (token == "concat_csa_files") {
      std::vector<std::string> strongPlayers = {
        "Gikou_20151118",
        "Gikou_20151122",
        "Gikou_20151117",
        "fib",
        "Maladies",
        "IOFJ",
        "ponanza-990XEE",
        "FGM",
        "VBV",
        "NineDayFever_XeonE5-2690_16c",
        "NXF",
        "Apery_Twig_i7-5820K",
        "Mignon",
        "YGDS",
        "ueueue",
        "Apery_twig_i7_6700K",
        "ToT",
        "NEF",
        "Apery_Twig_4790K_test1",
        "-w-",
        "Apery_Twig_i7-5960X_8c",
        "SOIPJD",
        "Apery_Twig_test",
        "OIDH",
        "TvT",
        "FF",
        "Raistlin",
        "AaA",
        "GSIOU",
        "xuishl",
        "icecream",
        "NOOOO",
        "Apery_i7_2700K",
        "Apery_5820K",
        "UNKO",
        "Apery_i7-5820",
        "Apery_GPSfish_4c",
        "-_-",
        "HGDKJ",
        "Bonafish_0.41",
        "DXV",
        "ycas",
        "vibgyor",
        "KSU",
        "aperyyyyyy",
        "Bonafish_0.42",
        "cvHIRAOKA",
        "XGI",
        "GIU",
        "vibes",
        "HIRAOKA",
        "HJK",
        "Apery_i7-4790k_4c",
        "TDA",
        "FI",
        "KHW",
        "hydrangea",
        "gaeouh",
        "XDKH",
        "DSUIHC",
        "tanuki-gcc_i7-5960X_8c",
        "AperyTwig_4790k_4c",
        "AUJK",
        "zako",
        "GPPSfish_minimal_5820K",
        "ap_p2c",
        "YNL",
        "Apery_Twig",
        "sinbo",
        "PXW",
        "AperyWCSC25_test1",
        "UIHPO",
        "-q-",
        "AperyTwig_5500U",
        "DIUH",
        "Cocoon",
        "Apery_Twig_sse42_980X",
        "nozomi_i7-4790",
        "gpsfish_XeonX5680_12c_bid",
        "Apery_WCSC25_sse42_980X",
        "tanukiclang-1c",
        "Apery_sse4.1msvc_8c",
        "Apery_20151016",
        "JDAS",
        "ponaX_test",
        "A-Sky",
        "KSHLK",
        "gpsfish_mini",
        "CheeCamembert",
        "Apery_i7-6700HQ_4c",
        "38GINApery_Twig",
        "gpsfish_Corei7-4771_4c",
        "Titanda_L",
        "ApeTW_Pack_EC2Win16-8",
        "patient",
        "7610_W",
        "Apery_i5-4670",
        "Apery_MacBookPro_Mid2015",
        "gpsfish_XeonX5680_12c",
        "Bonafish_0.39",
        "Apery_WCSC25_2c",
        "TUKASA_AOI",
        "tanuki-_5500U",
        "tanuki-4770K-",
        "Apery_20150909_i7-2600K",
        "yocvp13mk2",
        "stap5",
      };
      std::vector<GameRecord> gameRecords;
      csa::readCsas(
        "C:\\shogi\\floodgate\\wdoor2021",
        [strongPlayers](const std::filesystem::path& p) {
              return true;
      },
        [](const GameRecord& gameRecord) {
        return gameRecord.winner == 1 || gameRecord.winner == 2;
      },
        gameRecords);
      csa::writeCsa1("C:\\shogi\\floodgate\\wdoor.csa1", gameRecords);
      std::cout << "Finished..." << std::endl;
    }
    else if (token == "merge_csa_files") {
      csa::mergeCsa1s({
        "C:\\shogi\\floodgate\\2chkifu_csa\\2chkifu.csa1",
        "C:\\shogi\\floodgate\\wdoor.csa1" },
        "C:\\shogi\\floodgate\\merged.csa1",
        pos);
      std::cout << "Finished..." << std::endl;
    }
    else if (token == "extract_tanuki_lose") {
      std::vector<GameRecord> gameRecords;
      csa::readCsas(
        "C:\\shogi\\floodgate",
        [](const std::filesystem::path& p) {
        std::string str = p.string();
        return str.find("tanuki-") != std::string::npos;
      },
        [](const GameRecord& gameRecord) {
        return (gameRecord.blackPlayerName.find("tanuki-") != std::string::npos && gameRecord.winner == 2) ||
          (gameRecord.whitePlayerName.find("tanuki-") != std::string::npos && gameRecord.winner == 1);
      },
        gameRecords);
      csa::writeCsa1("C:\\shogi\\floodgate\\tanuki-lose.csa1", gameRecords);
      std::cout << "Finished..." << std::endl;
    }
    else if (token == "convert_to_sfen") {
      std::vector<GameRecord> gameRecords;
      csa::readCsa1("C:\\shogi\\floodgate\\wdoor.csa1", pos, gameRecords);
      std::ofstream ofs("C:\\shogi\\floodgate\\wdoor.sfen");
      int counter = 0;
      for (const auto& gameRecord : gameRecords) {
        if (++counter % 1000 == 0) {
          std::cout << counter << std::endl;
        }
        pos.set(USI::DefaultStartPositionSFEN, Threads.main());

        ofs << "startpos moves";
        for (const auto& move : gameRecord.moves) {
          ofs << " " << move.toUSI();
        }
        ofs << std::endl;
      }
    }
#endif
#endif
    else { SYNCCOUT << "unknown command: " << cmd << SYNCENDL; }
  } while (token != "quit" && argc == 1);

  if (Options[USI::OptionNames::WRITE_SYNTHESIZED_EVAL])
    Evaluater::writeSynthesized(Options[USI::OptionNames::EVAL_DIR]);

  Threads.main()->wait_for_search_finished();
}

std::string USI::score(Score score, Score alpha, Score beta)
{
  std::stringstream ss;

  assert(score != ScoreNone);

  if (isMate(score)) {
    // 詰み
    // mate の後には、何手で詰むかを表示する。
    ss << "mate " << (ScoreMate0Ply - score);
  }
  else if (isMated(score)) {
    // 詰まされている
    // mate の後には、何手で詰むかを表示する。
    ss << "mate " << (ScoreMated0Ply - score);
  }
  else if (isSuperior(score)) {
    // 優等局面
    // mate の後には、何手優等局面が続くか1000足して表示する
    ss << "mate " << (ScoreSuperior0Ply - score + 1000);
  }
  else if (isInferior(score)) {
    // 劣等局面
    // mate の後には、何手劣等局面が続くか1000引いて表示する
    ss << "mate " << (ScoreInferior0Ply - score - 1000);
  }
  else if (abs(score) < ScoreSuperiorMaxPly) {
    // cp は centi pawn の略
    int normalizedScore = score * 100 / PawnScore;
    ss << "cp " << normalizedScore;
  }

  ss << (beta <= score ? " lowerbound" : score <= alpha ? " upperbound" : "");

  return ss.str();
}

std::string USI::score(Score score)
{
  return USI::score(score, -ScoreInfinite, ScoreInfinite);
}
