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
              "dlshogi_15b_pre5_a100x8",
              "Neo.BURNING_BRIDGE_test210708",
              "Suisho4tsec2_TR3990X",
              "dlshogi_15b_pre_a100x8",
              "dlshogi_15b_pre6_a100x8",
              "havensgate_RyzenTR3990X",
              "Suisho210616_TR3990X",
              "BURNING_BRIDGE_tsec02_CoolDown",
              "Suisho3kai_TR3990X",
              "BLUETRANSPARENCY",
              "Incinerator",
              "Kamuy_s2_RyzenTR-3990X",
              "Suisho4_TR3990X",
              "BURNING_BRIDGES",
              "NEEDLED-35.8kai20x32_i9-7980XE",
              "dlshogi_WCSC31_RTX3070",
              "Suisho4test_TR3990X",
              "Yashajin_Ai",
              "BURNING_BRIDGE_210404z",
              "gct_model-0000225kai_D_v100x8",
              "gct_m-0000225kai_b256_RTX2080ti",
              "nnue_Ryzen5-5600X",
              "Suisho210701_TR3990X",
              "dlshogitest_mate3_pvm_v100x8",
              "dlshogi_15b_pre4_a100x8",
              "BURNING_BRIDGE_210529",
              "BBridges_i7-10750H",
              "Ryfamate_wcsc31_TR2950X-RTX3090",
              "gct_model-0000225kai_v100x8",
              "Suisho4_FVSCALE24_3990X",
              "DLSuisho079_RTX3090",
              "Pepelka",
              "dlshogitest_pv_mate_v100x8",
              "QueenAI_2105150108_i9-7920x",
              "Aschenputtel",
              "neodaigo",
              "gct_model-0000214_a100x8",
              "FukauraOuV700dev1_RTX3090-2080S",
              "Razoback",
              "ANESIS",
              "BURNING_BRIDGE_210701",
              "Suisho4_FVSCALE32_3990X",
              "BB_denryu2-Re5_i9-9960X_16c",
              "Frozen",
              "EXTREME",
              "DLSuisho065FO6.03_RTX3090",
              "Legoshi",
              "FukauraOuV700dev1",
              "Neo.BURNING_BRIDGE_test210506",
              "ncp_v03",
              "dlshogi_15b_pre2_v100x8",
              "Mariel",
              "DeepYO_WCSC31_RTX3090-2080S",
              "dlshogitest_pvm_226mix_v100x8",
              "LUNA",
              "Daigorilla2_test_E5_2698v4",
              "SNN_210511_8t",
              "ncp_v01",
              "BLUE_IMPACT",
              "BlackCat_032fv07_i9-7980XE_18c",
              "BURNING_BRIDGE_210401",
              "gct_m-0000311_RTX2080ti",
              "BB-HKPE9_TR3990X",
              "Qhapaq_from_NeoSaitama_RZ7_3800X",
              "gct_m-279_pv_mate_RTX2080ti",
              "Qhapaq_WCSC29_8c",
              "FukauraOu_RTX3090-2080S",
              "F-34K",
              "Kristallweizen_R9-3950X",
              "NEEDLED-35.8kai1361_Ryzen7-4800H",
              "xgs",
              "gcttest_x6_v3_RTX2080ti",
              "Daigorilla_E5_2698v4",
              "DG_test210419",
              "daigo8",
              "QueenInLove-t030_i9-7980XE_18c",
              "DLSuisho059_RTX3090",
              "Ciel",
              "Kristallweizen-TR2990WX",
              "DLSuisho055_RTX3090",
              "Megaptera",
              "gct_model-0000226kai_a100x8",
              "tanukichi_3990X",
              "BB_RTX3070",
              "nibanshibori_wcsc31b",
              "DLSuishoFO6.02_RTX3090",
              "sui3k_ry4500U",
              "gct_model-0000225kai_D_RTX2080ti",
              "gct_m-0000279_RTX2080ti",
              "jcas",
              "Kristffersen",
              "AAV",
              "CR7",
              "DaigorillaEX_test1_E5_2698v4",
              "daigorilla_test",
              "40b_n010_RTX3090",
              "gct_model-0000225kai_RTX2080ti",
              "Cendrillon",
              "Qhapaq_WCSC28_Mizar_4790k",
              "daigorilla_tsec02",
              "BB_ry4500U",
              "QueenAI_2105130336_i9-7920x",
              "Cendrillon_Ryzen7-4800H",
              "moonlight_ray",
              "QueenInLove-m047_Ryzen7-4800H",
              "mytest0426",
              "dlshogi_with_gct_055_v100x8",
              "DG_test210307_24C_ubuntu",
              "QueenInLove-j088_Ryzen7-4800H",
              "s3k9_i9-7980XE_18c",
              "nibanshibori_tsec2",
              "gct_m-0000297_RTX2080ti",
              "SNN_210511_4t",
              "NEEDLED-35.8kai18c133_R7-4800H",
              "RINGO",
              "Karimero",
              "DLSuisho083_RTX3090",
              "daigorilla",
              "ToraCat_010_i9-7980XE_18c",
              "PX-2",
              "nibanshibori_wcsc31b_V100x4",
              "PP1PIREMIA2001",
              "DLSuisho064_RTX3090",
              "QueenAI_210504c_i9-7920x",
              "gct_m-0000321_RTX2080ti",
              "d10_n002x_2080Ti",
              "KASHMIR",
              "frkr",
              "gct_m-0000225kai_b256_RTX2070_MQ",
              "WDSa050_Ryzen7-4800H",
              "Peach",
              "DLSuisho053_RTX3090",
              "nibanshibori_wcsc31c",
              "tttaki_kai",
              "40b_n010_RTX3070",
              "ELLE",
              "Lladro",
              "40b_n015d_RTX3090",
              "gcttest_x5_RTX2080ti",
              "Amaeta_Denryu1_2950X",
              "mbk_3340m",
              "40b_n018d_RTX3090",
              "Supercell",
              "ncp_v04",
              "40b_n014_RTX3070",
              "gct_m-297_pv_mate_RTX2080ti",
              "ECLIPSE_20210326",
              "40b_n017d_RTX3070",
              "gct_m-0000349_RTX2080ti",
              "usa2xkai-5950X-16T",
              "Suisho3test_i5-10210U",
              "gcttest_x7_v11_RTX2070_Max-Q",
              "JK18_4t",
              "Suisho3kai_YO6.01_i5-6300U",
              "Daigorilla_test_E5_2698v4",
              "40b_n007_RTX3070",
              "gct_m-0000225kai_b64_RTX2070_MQ",
              "Nao.",
              "wcs01",
              "Pinocchio",
              "40b_n008_RTX3070",
              "NEEDLED-35.8kai18e039_R7-4800H",
              "Rinne_x022",
              "gct_m-0000261_RTX2080ti",
              "40b_n002",
              "Takeshi2_Ryzen7-4800H",
              "gct_m-0000239_re_RTX2080ti",
              "NovemberRain",
              "gct_m-0000217_re_RTX2070_MQ",
              "gct_model-0000225kai_RTX2070_MQ",
              "Kristallweizen_i7-8750H",
              "40b_n003",
              "deg",
              "40b_n016b_RTX3070",
              "gct_m-0000225kai_b64_RTX2080ti",
              "40b_n006_RTX3090",
              "Aquarius-c021_i9-9960X_16c",
              "gct_m-0000237_re_RTX2080ti",
              "d_x15_n001_2080Ti",
              "Kristallweizen-E5-2620",
              "40b_n009_RTX3070",
              "gct_m-0000237_re_RTX2070_MQ",
              "ECLIPSE_denryu01",
              "gcttest_x7_v8_RTX2070_Max-Q",
              "yuatan",
              "Sagittarius",
              "momoka174",
              "nd3k18f",
              "40b_n016d_RTX3090",
              "Suisho2test_i5-10210U",
              "Mike_Cat",
              "Daigorilla_210522_E5_2698v4",
              "TEMPVSFVGIT_11",
              "QAIHKPE9",
              "Suisho4test_Ryzen5800HS",
              "Pacman_4",
              "40b_n012_RTX3070",
              "40-05-70",
              "action.RTX2070.MAX-Q",
              "RUN",
              "momoka0101",
              "d_x15_n002_2080Ti",
              "PLX",
              "W_0",
              "PX-1",
              "20b_n016_2080Ti",
              "30b_n009_RTX3070",
              "Kristallweizen-i7-6700HQ",
              "Aquarius",
              "40b_n011_RTX3070",
              "gct_m-0000205kai_RTX2070_MQ",
              "Takeshi_ry4500U",
              "gravitation",
              "VIVI006",
              "zzz",
              "usa2xkai-5950X-24T",
              "sui3k_2c",
              "gct_m-0000322_RTX2080ti",
            };
            std::vector<GameRecord> gameRecords;
            csa::readCsas(
                "C:\\shogi\\floodgate",
                [strongPlayers](const std::filesystem::path& p) {
                    std::string str = p.string();
                    return std::count_if(
                        strongPlayers.begin(),
                        strongPlayers.end(),
                        [&str](const auto& strongPlayer) {
                            return str.find("+" + strongPlayer + "+") != std::string::npos;
                        }) == 2;
                    //return true;
                },
                [](const GameRecord& gameRecord) {
                    //return gameRecord.winner == 1 || gameRecord.winner == 2;
                    return true;
                },
                    gameRecords);
            csa::writeCsa1("C:\\shogi\\floodgate\\wdoor.csa1", gameRecords);
            std::cout << "Finished..." << std::endl;
        }
        else if (token == "merge_csa_files") {
            csa::mergeCsa1s({
              "C:\\home\\develop\\shogi-kifu\\2chkifu_csa\\2chkifu.csa1",
              "C:\\home\\develop\\shogi-kifu\\wdoor.csa1" },
              "C:\\home\\develop\\shogi-kifu\\merged.csa1",
              pos);
            std::cout << "Finished..." << std::endl;
        }
        else if (token == "extract_tanuki_lose") {
            std::vector<GameRecord> gameRecords;
            csa::readCsas(
                "C:\\home\\develop\\shogi-kifu",
                [](const std::filesystem::path& p) {
                    std::string str = p.string();
                    return str.find("tanuki-") != std::string::npos;
                },
                [](const GameRecord& gameRecord) {
                    return (gameRecord.blackPlayerName.find("tanuki-") != std::string::npos && gameRecord.winner == 2) ||
                        (gameRecord.whitePlayerName.find("tanuki-") != std::string::npos && gameRecord.winner == 1);
                },
                    gameRecords);
            csa::writeCsa1("C:\\home\\develop\\shogi-kifu\\tanuki-lose.csa1", gameRecords);
            std::cout << "Finished..." << std::endl;
        }
        else if (token == "convert_to_sfen") {
            std::vector<GameRecord> gameRecords;
            csa::readCsa1("C:\\shogi\\floodgate\\wdoor.csa1", pos, gameRecords);
            std::ofstream ofs("C:\\shogi\\floodgate\\startpos.2021-07-13.only_strong.sfen");
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
