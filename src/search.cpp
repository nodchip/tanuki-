#include "timeManager.hpp"
#include "search.hpp"

#include "generateMoves.hpp"
#include "parameters.hpp"
#include "usi.hpp"

namespace Search {

  SignalsType Signals;
  LimitsType Limits;
  StateStackPtr SetupStates;
  Book book;
  std::mutex BroadcastMutex;
  int BroadcastPvDepth;
  std::string BroadcastPvInfo;
}

using namespace Search;

namespace {

  // Ponder用の指し手
  Move ponderCandidate;

  // Different node types, used as template parameter
  enum NodeType { Root, PV, NonPV };

  static constexpr Score INITIAL_ASPIRATION_WINDOW_WIDTH = (Score)16;
  static constexpr Score SECOND_ASPIRATION_WINDOW_WIDTH = (Score)64;

  inline Score razorMargin(const Depth d) {
    return static_cast<Score>((SEARCH_RAZORING_MARGIN_SLOPE * d
      + SEARCH_RAZORING_MARGIN_INTERCEPT) / FLOAT_SCALE);
  }

  Score FutilityMargins[16][64]; // [depth][moveCount]
  inline Score futilityMargin(const Depth depth, const int moveCount) {
    return (depth < static_cast<Depth>(SEARCH_FUTILITY_MARGIN_DEPTH_THRESHOLD) ?
      FutilityMargins[std::max(depth, Depth1)][std::min(moveCount, 63)]
      : 2 * ScoreInfinite);
  }

  int FutilityMoveCounts[32];    // [depth]

  s8 Reductions[2][64][64]; // [pv][depth][moveNumber]
  template <bool PVNode> inline Depth reduction(const Depth depth, const int moveCount) {
    return static_cast<Depth>(Reductions[PVNode][std::min(Depth(static_cast<int>(depth) / static_cast<int>(OnePly)), Depth(63))][std::min(moveCount, 63)]);
  }

  // EasyMoveManager struct is used to detect a so called 'easy move'; when PV is
  // stable across multiple search iterations we can fast return the best move.
  struct EasyMoveManager {

    void clear() {
      stableCnt = 0;
      expectedPosKey = Key();
      pv[0] = pv[1] = pv[2] = Move::moveNone();
    }

    Move get(Key key) const {
      return expectedPosKey == key ? pv[2] : Move::moveNone();
    }

    void update(Position& pos, const std::vector<Move>& newPv) {

      assert(newPv.size() >= 3);

      // Keep track of how many times in a row 3rd ply remains stable
      stableCnt = (newPv[2] == pv[2]) ? stableCnt + 1 : 0;

      if (!std::equal(newPv.begin(), newPv.begin() + 3, pv))
      {
        std::copy(newPv.begin(), newPv.begin() + 3, pv);

        StateInfo st[2];
        pos.doMove(newPv[0], st[0]);
        pos.doMove(newPv[1], st[1]);
        expectedPosKey = pos.getKey();
        pos.undoMove(newPv[1]);
        pos.undoMove(newPv[0]);
      }
    }

    int stableCnt;
    Key expectedPosKey;
    Move pv[3];
  };

  EasyMoveManager EasyMove;

  template <NodeType NT>
  Score search(Position& pos, Search::SearchStack* ss, Score alpha, Score beta, Depth depth, bool cutNode);

  template <NodeType NT, bool InCheck>
  Score qsearch(Position& pos, Search::SearchStack* ss, Score alpha, Score beta, Depth depth);

  Score score_to_tt(Score s, int ply);
  Score score_from_tt(Score s, int ply);
  void update_pv(Move* pv, Move move, Move* childPv);
  void update_stats(const Position& pos, Search::SearchStack* ss, Move move, Depth depth, Move* quiets, int quietsCnt);
  void check_time();
  // 入玉勝ちかどうかを判定
  bool nyugyoku(const Position& pos);
} // namespace


  /// Search::init() is called during startup to initialize various lookup tables

void Search::init() {
  // todo: パラメータは改善の余地あり。
  int d;  // depth (ONE_PLY == 2)
  int hd; // half depth (ONE_PLY == 1)
  int mc; // moveCount

          // Init reductions array
  for (hd = 1; hd < 64; hd++) {
    for (mc = 1; mc < 64; mc++) {
      double pvRed =
        (SEARCH_FUTILITY_PRUNING_PV_REDUCTION_SLOPE * log(double(hd)) * log(double(mc))
          + SEARCH_FUTILITY_PRUNING_PV_REDUCTION_INTERCEPT) / FLOAT_SCALE;
      double nonPVRed =
        (SEARCH_FUTILITY_PRUNING_NON_PV_REDUCTION_SLOPE * log(double(hd)) * log(double(mc))
          + SEARCH_FUTILITY_PRUNING_NON_PV_REDUCTION_INTERCEPT) / FLOAT_SCALE;
      Reductions[1][hd][mc] = (int8_t)(pvRed >= 1.0 ? floor(pvRed * int(OnePly)) : 0);
      Reductions[0][hd][mc] = (int8_t)(nonPVRed >= 1.0 ? floor(nonPVRed * int(OnePly)) : 0);
    }
  }

  for (d = 1; d < 16; ++d) {
    for (mc = 0; mc < 64; ++mc) {
      FutilityMargins[d][mc] = static_cast<Score>(static_cast<int>((
        SEARCH_FUTILITY_MARGIN_LOG_D_COEFFICIENT * log(d)
        - SEARCH_FUTILITY_MARGIN_MOVE_COUNT_COEFFICIENT * mc
        - SEARCH_FUTILITY_MARGIN_INTERCEPT) / FLOAT_SCALE));
    }
  }

  // init futility move counts
  for (d = 0; d < 32; ++d) {
    FutilityMoveCounts[d] =
      (SEARCH_FUTILITY_MOVE_COUNTS_SCALE
        * static_cast<int>(pow(d, SEARCH_FUTILITY_MOVE_COUNTS_POWER / static_cast<double>(FLOAT_SCALE)))
        + SEARCH_FUTILITY_MOVE_COUNTS_INTERCEPT)
      / FLOAT_SCALE;
  }

  Options.init();
  Threads.init();
  TT.resize(Options[USI::OptionNames::USI_HASH]);
}


/// Search::clear() resets to zero search state, to obtain reproducible results

void Search::clear() {

  TT.clear();

  for (Thread* th : Threads)
  {
    th->history.clear();
    th->gains.clear();
  }
}

/// MainThread::search() is called by the main thread when the program receives
/// the UCI 'go' command. It searches from root position and at the end prints
/// the "bestmove" to output.

void MainThread::search() {
  Color us = rootPos.turn();
  Time.init(Limits, us, rootPos.gamePly());

  bool nyugyokuWin = false;
  if (nyugyoku(rootPos)) {
    nyugyokuWin = true;
  }
  else if (rootMoves.empty()) {
    // 指し手がなければ負け
    rootMoves.push_back(RootMove(Move::moveNone()));
    if (Options[USI::OptionNames::OUTPUT_INFO]) {
      SYNCCOUT << "info depth 0 score "
        << USI::score(-ScoreMate0Ply)
        << SYNCENDL;
    }
  }
  else
  {
    // 定跡データベースのlookup
    auto bookMove = book.probe(rootPos);
    if (bookMove.first != Move::moveNone()) {
      const auto& move = bookMove.first;
      const auto& score = bookMove.second;

      // rootMovesを1手だけにし、スコアを付加する
      rootMoves.clear();
      rootMoves.push_back(RootMove(move));
      rootMoves[0].score = score;

      // rootMoves等をヘルパースレッド伝搬する
      for (Thread* th : Threads)
      {
        th->maxPly = 0;
        th->rootDepth = Depth0;
        if (th != this)
        {
          th->rootPos = Position(rootPos, th);
          th->rootMoves = rootMoves;
        }
      }

      if (Options[USI::OptionNames::OUTPUT_INFO]) {
        SYNCCOUT << "info depth 24"
          << " score " << USI::score(score)
          << " pv " << move.toUSI()
          << SYNCENDL;
      }

      int bookSleepTime = Options[USI::OptionNames::BOOK_SLEEP_TIME];
      std::chrono::milliseconds dura(bookSleepTime);
      std::this_thread::sleep_for(dura);
    }
    else {
      for (Thread* th : Threads)
      {
        th->maxPly = 0;
        th->rootDepth = Depth0;
        if (th != this)
        {
          th->rootPos = Position(rootPos, th);
          th->rootMoves = rootMoves;
          th->start_searching();
        }
      }

      Thread::search(); // Let's start searching!
    }
  }

  // When playing in 'nodes as time' mode, subtract the searched nodes from
  // the available ones before to exit.
  if (Limits.npmsec)
    Time.availableNodes += Limits.inc[us] - Threads.nodes_searched();

  // When we reach the maximum depth, we can arrive here without a raise of
  // Signals.stop. However, if we are pondering or in an infinite search,
  // the UCI protocol states that we shouldn't print the best move before the
  // GUI sends a "stop" or "ponderhit" command. We therefore simply wait here
  // until the GUI sends one of those commands (which also raises Signals.stop).
  if (!Signals.stop && (Limits.ponder || Limits.infinite))
  {
    Signals.stopOnPonderhit = true;
    wait(Signals.stop);
  }

  // Stop the threads if not already stopped
  Signals.stop = true;

  // Wait until all threads have finished
  for (Thread* th : Threads)
    if (th != this)
      th->wait_for_search_finished();

  // Check if there are threads with a better score than main thread
  Thread* bestThread = this;
  if (!this->easyMovePlayed
    &&  Options[USI::OptionNames::MULTIPV] == 1)
  {
    for (Thread* th : Threads)
      if (th->completedDepth > bestThread->completedDepth
        && th->rootMoves[0].score > bestThread->rootMoves[0].score)
        bestThread = th;
  }

  // Send new PV when needed
  if (bestThread != this) {
    if (Options[USI::OptionNames::OUTPUT_INFO]) {
      SYNCCOUT << USI::pv(bestThread->rootPos, bestThread->completedDepth, -ScoreInfinite, ScoreInfinite) << SYNCENDL;
    }
  }

  if (nyugyokuWin) {
    SYNCCOUT << "bestmove win" << SYNCENDL;
  }
  else if (rootMoves[0].pv[0].isNone()) {
    // 指し手がない場合、投了する
    SYNCCOUT << "bestmove resign" << SYNCENDL;
  }
  else {
    SYNCCOUT << "bestmove " << bestThread->rootMoves[0].pv[0].toUSI();
    if (bestThread->rootMoves[0].pv.size() > 1 || bestThread->rootMoves[0].extract_ponder_from_tt(rootPos, ponderCandidate))
      std::cout << " ponder " << bestThread->rootMoves[0].pv[1].toUSI();
    std::cout << SYNCENDL;
  }
}


// Thread::search() is the main iterative deepening loop. It calls search()
// repeatedly with increasing depth until the allocated thinking time has been
// consumed, user stops the search, or the maximum search depth is reached.

void Thread::search() {

  Search::SearchStack stack[MaxPly + 4], *ss = stack + 2; // To allow referencing (ss-2) and (ss+2)
  Score bestScore, alpha, beta, delta;
  Move easyMove = Move::moveNone();
  MainThread* mainThread = (this == Threads.main() ? Threads.main() : nullptr);

  std::memset(ss - 2, 0, 5 * sizeof(Search::SearchStack));

  bestScore = delta = alpha = -ScoreInfinite;
  beta = ScoreInfinite;
  completedDepth = Depth0;

  if (mainThread)
  {
    easyMove = EasyMove.get(rootPos.getKey());
    EasyMove.clear();
    mainThread->easyMovePlayed = mainThread->failedLow = false;
    mainThread->bestMoveChanges = 0;

    // ponder用の指し手の初期化
    ponderCandidate = Move::moveNone();

    TT.new_search();
  }

  size_t multiPV = Options[USI::OptionNames::MULTIPV];

  multiPV = std::min(multiPV, rootMoves.size());

  // Iterative deepening loop until requested to stop or target depth reached
  while (++rootDepth < DepthMax && !Signals.stop && (!Limits.depth || rootDepth <= Limits.depth))
  {
    // Set up the new depth for the helper threads skipping in average each
    // 2nd ply (using a half density map similar to a Hadamard matrix).
    if (!mainThread)
    {
      int d = rootDepth + rootPos.gamePly();

      if (idx <= 6 || idx > 24)
      {
        if (((d + idx) >> (msb(idx + 1) - 1)) % 2)
          continue;
      }
      else
      {
        // Table of values of 6 bits with 3 of them set
        static const int HalfDensityMap[] = {
          0x07, 0x0b, 0x0d, 0x0e, 0x13, 0x16, 0x19, 0x1a, 0x1c,
          0x23, 0x25, 0x26, 0x29, 0x2c, 0x31, 0x32, 0x34, 0x38
        };

        if ((HalfDensityMap[idx - 7] >> (d % 6)) & 1)
          continue;
      }

      {
        // broadcastされてきたdepth以下はスキップする
        std::lock_guard<std::mutex> lock(Search::BroadcastMutex);
        if (rootDepth <= Search::BroadcastPvDepth) {
          continue;
        }
      }
    }

    // Age out PV variability metric
    if (mainThread)
      mainThread->bestMoveChanges *= 0.505, mainThread->failedLow = false;

    // Save the last iteration's scores before first PV line is searched and
    // all the move scores except the (new) PV are set to -VALUE_INFINITE.
    for (RootMove& rm : rootMoves)
      rm.previousScore = rm.score;

    // MultiPV loop. We perform a full root search for each PV line
    for (PVIdx = 0; PVIdx < multiPV && !Signals.stop; ++PVIdx)
    {
      // Reset aspiration window starting size
      if (rootDepth >= 5)
      {
        delta = INITIAL_ASPIRATION_WINDOW_WIDTH;
        alpha = std::max(rootMoves[PVIdx].previousScore - delta, -ScoreInfinite);
        beta = std::min(rootMoves[PVIdx].previousScore + delta, ScoreInfinite);
      }

      // Start with a small aspiration window and, in the case of a fail
      // high/low, re-search with a bigger window until we're not failing
      // high/low anymore.
      while (true)
      {
        ss->staticEvalRaw.p[0][0] = (ss + 1)->staticEvalRaw.p[0][0] = ScoreNotEvaluated;
        bestScore = ::search<Root>(rootPos, ss + 1, alpha, beta, rootDepth * OnePly, false);

        // Bring the best move to the front. It is critical that sorting
        // is done with a stable algorithm because all the values but the
        // first and eventually the new best one are set to -VALUE_INFINITE
        // and we want to keep the same order for all the moves except the
        // new PV that goes to the front. Note that in case of MultiPV
        // search the already searched PV lines are preserved.
        std::stable_sort(rootMoves.begin() + PVIdx, rootMoves.end());

        // Write PV back to transposition table in case the relevant
        // entries have been overwritten during the search.
        for (size_t i = 0; i <= PVIdx; ++i) {
          ss->staticEvalRaw.p[0][0] = (ss + 1)->staticEvalRaw.p[0][0] = ScoreNotEvaluated;
          rootMoves[i].insert_pv_in_tt(rootPos);
        }

        // If search has been stopped break immediately. Sorting and
        // writing PV back to TT is safe because RootMoves is still
        // valid, although it refers to previous iteration.
        if (Signals.stop)
          break;

        if (!mainThread) {
          // broadcastされてきたdepth以下はスキップする
          std::lock_guard<std::mutex> lock(Search::BroadcastMutex);
          if (rootDepth <= Search::BroadcastPvDepth) {
            break;
          }
        }

        // When failing high/low give some update (without cluttering
        // the UI) before a re-search.
        if (mainThread
          && multiPV == 1
          && (bestScore <= alpha || bestScore >= beta)
          && Time.elapsed() > 3000) {
          if (Options[USI::OptionNames::OUTPUT_INFO]) {
            SYNCCOUT << USI::pv(rootPos, rootDepth, alpha, beta) << SYNCENDL;
          }
        }

        if (delta == INITIAL_ASPIRATION_WINDOW_WIDTH) {
          delta = SECOND_ASPIRATION_WINDOW_WIDTH;
        }

        // In case of failing low/high increase aspiration window and
        // re-search, otherwise exit the loop.
        if (bestScore <= alpha)
        {
          beta = (alpha + beta) / 2;
          alpha = std::max(bestScore - delta, -ScoreInfinite);

          if (mainThread)
          {
            mainThread->failedLow = true;
            Signals.stopOnPonderhit = false;
          }
        }
        else if (bestScore >= beta)
        {
          alpha = (alpha + beta) / 2;
          beta = std::min(bestScore + delta, ScoreInfinite);
        }
        else
          break;

        delta += delta / 2;

        assert(alpha >= -ScoreInfinite && beta <= ScoreInfinite);
      }

      // Sort the PV lines searched so far and update the GUI
      std::stable_sort(rootMoves.begin(), rootMoves.begin() + PVIdx + 1);

      if (!mainThread)
        break;

      if (Signals.stop) {
        if (Options[USI::OptionNames::OUTPUT_INFO]) {
          SYNCCOUT << "info nodes " << Threads.nodes_searched()
            << " time " << Time.elapsed() << SYNCENDL;
        }
      }

      else if (PVIdx + 1 == multiPV || Time.elapsed() > 3000) {
        if (Options[USI::OptionNames::OUTPUT_INFO]) {
          SYNCCOUT << USI::pv(rootPos, rootDepth, alpha, beta) << SYNCENDL;
        }
      }

      if (!mainThread) {
        // broadcastされてきたdepth以下はスキップする
        std::lock_guard<std::mutex> lock(Search::BroadcastMutex);
        if (rootDepth <= Search::BroadcastPvDepth) {
          break;
        }
      }
    }

    if (!Signals.stop) {
      completedDepth = rootDepth;

      {
        std::lock_guard<std::mutex> lock(Search::BroadcastMutex);
        Search::BroadcastPvDepth = std::max(Search::BroadcastPvDepth, static_cast<int>(completedDepth));
      }
    }

    if (!mainThread)
      continue;

    // Have we found a "mate in x"?
    if (Limits.mate
      && bestScore >= ScoreMateInMaxPly
      && ScoreMate0Ply - bestScore <= 2 * Limits.mate)
      Signals.stop = true;

    // ponder用の指し手として、2手目の指し手を保存しておく。
    // これがmain threadのものだけでいいかどうかはよくわからないが。
    // とりあえず、無いよりマシだろう。
    if (mainThread->rootMoves[0].pv.size() > 1) {
      ponderCandidate = mainThread->rootMoves[0].pv[1];
    }

    // Do we have time for the next iteration? Can we stop searching now?
    if (Limits.use_time_management())
    {
      if (!Signals.stop && !Signals.stopOnPonderhit)
      {
        // Take some extra time if the best move has changed
        if (rootDepth > 4 * OnePly && multiPV == 1)
          Time.pv_instability(mainThread->bestMoveChanges);

        // Stop the search if only one legal move is available or all
        // of the available time has been used or we matched an easyMove
        // from the previous search and just did a fast verification.
        //SYNCCOUT << "info string elapsed=" << Time.elapsed()
        //  << " available=" << Time.available()
        //  << " maximum=" << Time.maximum() << SYNCENDL;
        if (rootMoves.size() == 1
          || Time.elapsed() > Time.available() * (mainThread->failedLow ? 641 : 315) / 640
          || (mainThread->easyMovePlayed = (rootMoves[0].pv[0] == easyMove
            && mainThread->bestMoveChanges < 0.03
            && Time.elapsed() > Time.available() / 8)))
        {
          // If we are allowed to ponder do not stop the search now but
          // keep pondering until the GUI sends "ponderhit" or "stop".
          if (Limits.ponder)
            Signals.stopOnPonderhit = true;
          else
            Signals.stop = true;
        }

        if (rootMoves[0].pv.size() >= 3)
          EasyMove.update(rootPos, rootMoves[0].pv);
        else
          EasyMove.clear();
      }
    }
  }

  if (!mainThread)
    return;

  // Clear any candidate easy move that wasn't stable for the last search
  // iterations; the second condition prevents consecutive fast moves.
  if (EasyMove.stableCnt < 6 || mainThread->easyMovePlayed)
    EasyMove.clear();
}

template <bool DO> void Position::doNullMove(StateInfo& backUpSt) {
  assert(!inCheck());

  StateInfo* src = (DO ? st_ : &backUpSt);
  StateInfo* dst = (DO ? &backUpSt : st_);

  dst->boardKey = src->boardKey;
  dst->handKey = src->handKey;
  dst->pliesFromNull = src->pliesFromNull;
  dst->hand = hand(turn());
  turn_ = oppositeColor(turn());

  if (DO) {
    st_->boardKey ^= zobTurn();
    prefetch(TT.first_entry(st_->key()));
    st_->pliesFromNull = 0;
    st_->continuousCheck[turn()] = 0;
  }
  st_->hand = hand(turn());

  assert(isOK());
}

namespace {

  Depth clamp(Depth d, Depth lo, Depth hi)
  {
    assert(lo <= hi);
    if (d < lo) {
      return lo;
    }
    else if (hi < d) {
      return hi;
    }
    else {
      return d;
    }
  }

  // 1 ply前の first move によって second move が合法手にするか。
  bool allows(const Position& pos, const Move first, const Move second) {
    const Square m1to = first.to();
    const Square m1from = first.from();
    const Square m2from = second.from();
    const Square m2to = second.to();
    if (m1to == m2from || m2to == m1from) {
      return true;
    }

    if (second.isDrop() && first.isDrop()) {
      return false;
    }

    if (!second.isDrop() && !first.isDrop()) {
      if (betweenBB(m2from, m2to).isSet(m1from)) {
        return true;
      }
    }

    const PieceType m1pt = first.pieceTypeFromOrDropped();
    const Color us = pos.turn();
    const Bitboard occ = (second.isDrop() ? pos.occupiedBB() : pos.occupiedBB() ^ setMaskBB(m2from));
    const Bitboard m1att = pos.attacksFrom(m1pt, us, m1to, occ);
    if (m1att.isSet(m2to)) {
      return true;
    }

    if (m1att.isSet(pos.kingSquare(us))) {
      return true;
    }

    return false;
  }

  // fitst move によって、first move の相手側の second move を違法手にするか。
  bool refutes(const Position& pos, const Move first, const Move second) {
    assert(pos.isOK());

    const Square m2to = second.to();
    const Square m1from = first.from(); // 駒打でも今回はこれで良い。

    if (m1from == m2to) {
      return true;
    }

    const PieceType m2ptFrom = second.pieceTypeFrom();
    if (second.isCaptureOrPromotion()
      && ((pos.pieceScore(second.cap()) <= pos.pieceScore(m2ptFrom))
        || m2ptFrom == King))
    {
      // first により、新たに m2to に当たりになる駒があるなら true
      assert(!second.isDrop());

      const Color us = pos.turn();
      const Square m1to = first.to();
      const Square m2from = second.from();
      Bitboard occ = pos.occupiedBB() ^ setMaskBB(m2from) ^ setMaskBB(m1to);
      PieceType m1ptTo;

      if (first.isDrop()) {
        m1ptTo = first.pieceTypeDropped();
      }
      else {
        m1ptTo = first.pieceTypeTo();
        occ ^= setMaskBB(m1from);
      }

      if (pos.attacksFrom(m1ptTo, us, m1to, occ).isSet(m2to)) {
        return true;
      }

      const Color them = oppositeColor(us);
      // first で動いた後、sq へ当たりになっている遠隔駒
      const Bitboard xray =
        (pos.attacksFrom<Lance>(them, m2to, occ) & pos.bbOf(Lance, us))
        | (pos.attacksFrom<Rook  >(m2to, occ) & pos.bbOf(Rook, Dragon, us))
        | (pos.attacksFrom<Bishop>(m2to, occ) & pos.bbOf(Bishop, Horse, us));

      // sq へ当たりになっている駒のうち、first で動くことによって新たに当たりになったものがあるなら true
      if (xray.isNot0() && (xray ^ (xray & queenAttack(m2to, pos.occupiedBB()))).isNot0()) {
        return true;
      }
    }

    if (!second.isDrop()
      && isSlider(m2ptFrom)
      && betweenBB(second.from(), m2to).isSet(first.to())
      && ScoreZero <= pos.seeSign(first))
    {
      return true;
    }

    return false;
  }

  // search<>() is the main search function for both PV and non-PV nodes

  template <NodeType NT>
  Score search(Position& pos, Search::SearchStack* ss, Score alpha, Score beta, Depth depth, bool cutNode) {

    constexpr bool PVNode = (NT == PV || NT == Root);
    constexpr bool RootNode = (NT == Root);

    assert(-ScoreInfinite <= alpha && alpha < beta && beta <= ScoreInfinite);
    assert(PVNode || (alpha == beta - 1));
    assert(Depth0 < depth);

    // 途中で goto を使用している為、先に全部の変数を定義しておいた方が安全。
    Move pv[MaxPly + 1];
    Move movesSearched[64];
    StateInfo st;
    TTEntry* tte;
    Key posKey;
    Move ttMove;
    Move move;
    Move excludedMove;
    Move bestMove;
    Move threatMove;
    Depth newDepth;
    Depth extension;
    Score bestScore;
    Score score;
    Score ttScore;
    Score eval;
    bool inCheck;
    bool givesCheck;
    bool singularExtensionNode;
    bool captureOrPawnPromotion;
    bool dangerous;
    bool doFullDepthSearch;
    int moveCount;
    int playedMoveCount;
    bool ttHit;

    // step1
    // initialize node
    Thread* thisThread = pos.thisThread();
    moveCount = playedMoveCount = ss->moveCount = 0;
    inCheck = pos.inCheck();

    bestScore = -ScoreInfinite;
    ss->currentMove = threatMove = bestMove = (ss + 1)->excludedMove = Move::moveNone();
    ss->ply = (ss - 1)->ply + 1;
    (ss + 1)->skipNullMove = false;
    (ss + 1)->reduction = Depth0;
    (ss + 2)->killers[0] = (ss + 2)->killers[1] = Move::moveNone();

    // Check for available remaining time
    if (thisThread->resetCalls.load(std::memory_order_relaxed))
    {
      thisThread->resetCalls = false;
      thisThread->callsCnt = 0;
    }
    if (++thisThread->callsCnt > 4096)
    {
      for (Thread* th : Threads)
        th->resetCalls = true;

      check_time();
    }

    if (PVNode && thisThread->maxPly < ss->ply) {
      thisThread->maxPly = ss->ply;
    }

    if (!RootNode) {
      // step2
      // stop と最大探索深さのチェック
      switch (pos.isDraw(16)) {
      case NotRepetition: if (!Signals.stop && ss->ply <= MaxPly) { break; }
      case RepetitionDraw: return ScoreDraw;
      case RepetitionWin: return mateIn(ss->ply);
      case RepetitionLose: return matedIn(ss->ply);
      case RepetitionSuperior: if (ss->ply != 2) { return superiorIn(ss->ply); } break;
      case RepetitionInferior: if (ss->ply != 2) { return inferiorIn(ss->ply); } break;
      default: UNREACHABLE;
      }

      // step3
      // mate distance pruning
      if (!RootNode) {
        alpha = std::max(matedIn(ss->ply), alpha);
        beta = std::min(mateIn(ss->ply + 1), beta);
        if (beta <= alpha) {
          return alpha;
        }
      }
    }

    pos.setNodesSearched(pos.nodesSearched() + 1);

    // step4
    // trans position table lookup
    excludedMove = ss->excludedMove;
    posKey = (excludedMove.isNone() ? pos.getKey() : pos.getExclusionKey());
    tte = TT.probe(posKey, ttHit);
    ttMove =
      RootNode ? thisThread->rootMoves[thisThread->PVIdx].pv[0] :
      ttHit ?
      move16toMove(tte->move(), pos) :
      Move::moveNone();
    ttScore = (ttHit ? score_from_tt(tte->score(), ss->ply) : ScoreNone);
    if (!((-ScoreInfinite < ttScore && ttScore < ScoreInfinite) || ttScore == ScoreNone)) {
      pos.print();
      std::cerr << __FILE__ << " " << __FUNCTION__ << " " << __LINE__
        << " ttScore=" << ttScore
        << " tte->score()=" << tte->score()
        << " ss->ply=" << ss->ply
        << std::endl;
      assert(false);
    }

    if (!PVNode        // PV nodeでは置換表の指し手では枝刈りしない(PV nodeはごくわずかしかないので..)
      && ttHit         // 置換表の指し手がhitして
      && tte->depth() >= depth   // 置換表に登録されている探索深さのほうが深くて
      && ttScore != ScoreNone   // (VALUE_NONEだとすると他スレッドからTTEntryが読みだす直前に破壊された可能性がある)
      && (ttScore >= beta ? (tte->bound() & BoundLower)
        : (tte->bound() & BoundUpper))
      // ttValueが下界(真の評価値はこれより大きい)もしくはジャストな値で、かつttValue >= beta超えならbeta cutされる
      // ttValueが上界(真の評価値はこれより小さい)だが、tte->depth()のほうがdepthより深いということは、
      // 今回の探索よりたくさん探索した結果のはずなので、今回よりは枝刈りが甘いはずだから、その値を信頼して
      // このままこの値でreturnして良い。
      )
    {
      //tt.refresh(tte);
      ss->currentMove = ttMove; // Move::moveNone() もありえる。

      if (beta <= ttScore
        && !ttMove.isNone()
        && !ttMove.isCaptureOrPawnPromotion()
        && ttMove != ss->killers[0])
      {
        ss->killers[1] = ss->killers[0];
        ss->killers[0] = ttMove;
      }
      if (!(-ScoreInfinite < ttScore && ttScore < ScoreInfinite)) {
        pos.print();
        std::cerr << __FILE__ << " " << __FUNCTION__ << " " << __LINE__
          << " ttScore=" << ttScore
          << std::endl;
        assert(false);
      }
      //assert(-ScoreInfinite < ttScore && ttScore < ScoreInfinite);
      return ttScore;
    }

    // 宣言勝ち
    {
      // 王手がかかってようがかかってまいが、宣言勝ちの判定は正しい。
      // (トライルールのとき王手を回避しながら入玉することはありうるので)
      bool nyugyokuWin = nyugyoku(pos);
      if (nyugyokuWin)
      {
        bestScore = mateIn(ss->ply + 1); // 1手詰めなのでこの次のnodeで(指し手がなくなって)詰むという解釈
        tte->save(posKey, score_to_tt(bestScore, ss->ply), BoundExact,
          DepthMax, Move::moveNone(), ss->staticEval, TT.generation());
        return bestScore;
      }
    }

#if 1
    if (!RootNode
      && !inCheck)
    {
      if (!(move = pos.mateMoveIn1Ply()).isNone()) {
        ss->staticEval = bestScore = mateIn(ss->ply);
        Score newTtScore = score_to_tt(bestScore, ss->ply);
        assert(-ScoreInfinite < newTtScore && newTtScore < ScoreInfinite);
        tte->save(posKey, newTtScore, BoundExact, depth,
          move, ss->staticEval, TT.generation());
        bestMove = move;
        assert(-ScoreInfinite < bestScore && bestScore < ScoreInfinite);
        return bestScore;
      }
    }
#endif

    // step5
    // evaluate the position statically
    eval = ss->staticEval = evaluate(pos, ss); // Bonanza の差分評価の為、evaluate() を常に呼ぶ。
    if (inCheck) {
      eval = ss->staticEval = ScoreNone;
      goto iid_start;
    }
    else if (ttHit) {
      if (ttScore != ScoreNone
        && (tte->bound() & (eval < ttScore ? BoundLower : BoundUpper)))
      {
        eval = ttScore;
      }
    }
    else {
      tte->save(posKey, ScoreNone, BoundNone, DepthNone,
        Move::moveNone(), ss->staticEval, TT.generation());
    }

    // step6
    // razoring
    if (!PVNode
      && depth < SEARCH_RAZORING_DEPTH
      && eval + razorMargin(depth) < beta
      && ttMove.isNone()
      && abs(beta) < ScoreMateInMaxPly)
    {
      const Score rbeta = beta - razorMargin(depth);
      const Score s = qsearch<NonPV, false>(pos, ss, rbeta - 1, rbeta, Depth0);
      if (s < rbeta) {
        assert(-ScoreInfinite < s && s < ScoreInfinite);
        return s;
      }
    }

    // step7
    // static null move pruning
    if (!PVNode
      && !ss->skipNullMove
      && depth < SEARCH_STATIC_NULL_MOVE_PRUNING_DEPTH_THRESHOLD
      && beta <= eval - FutilityMargins[depth][0]
      && abs(beta) < ScoreMateInMaxPly)
    {
      score = eval - FutilityMargins[depth][0];
      score = std::max(score, -ScoreInfinite + 1);
      score = std::min(score, ScoreInfinite - 1);
      assert(-ScoreInfinite < score && score < ScoreInfinite);
      return score;
    }

    // step8
    // null move
    if (!PVNode
      && !ss->skipNullMove
      && static_cast<Depth>(SEARCH_NULL_MOVE_DEPTH_THRESHOLD) <= depth
      && beta <= eval
      && abs(beta) < ScoreMateInMaxPly)
    {
      ss->currentMove = Move::moveNull();
      Depth reduction = (SEARCH_NULL_MOVE_REDUCTION_SLOPE * depth
        + SEARCH_NULL_MOVE_REDUCTION_INTERCEPT) / FLOAT_SCALE;

      if (beta < eval - SEARCH_NULL_MOVE_MARGIN) {
        reduction += OnePly;
      }
      Depth nextDepth = clamp(depth - reduction, Depth1, depth - Depth1);

      pos.doNullMove<true>(st);
      (ss + 1)->staticEvalRaw = (ss)->staticEvalRaw; // 評価値の差分評価の為。
      (ss + 1)->skipNullMove = true;
      Score nullScore;
      if (nextDepth < OnePly) {
        nullScore = -qsearch<NonPV, false>(pos, ss + 1, -beta, -alpha, Depth0);
      }
      else {
        assert(Depth0 < nextDepth);
        nullScore = -search<NonPV>(pos, ss + 1, -beta, -alpha, nextDepth, !cutNode);
      }
      (ss + 1)->skipNullMove = false;
      pos.doNullMove<false>(st);

      if (beta <= nullScore) {
        if (ScoreMateInMaxPly <= nullScore) {
          nullScore = beta;
        }

        if (depth < SEARCH_NULL_MOVE_NULL_SCORE_DEPTH_THRESHOLD) {
          assert(-ScoreInfinite < nullScore && nullScore < ScoreInfinite);
          return nullScore;
        }

        ss->skipNullMove = true;
        assert(Depth0 < nextDepth);
        const Score s = search<NonPV>(pos, ss, alpha, beta, nextDepth, false);
        ss->skipNullMove = false;

        if (beta <= s) {
          assert(-ScoreInfinite < nullScore && nullScore < ScoreInfinite);
          return nullScore;
        }
      }
      else {
        // fail low
        threatMove = (ss + 1)->currentMove;
        if (depth < SEARCH_NULL_FAIL_LOW_SCORE_DEPTH_THRESHOLD
          && (ss - 1)->reduction != Depth0
          && !threatMove.isNone()
          && allows(pos, (ss - 1)->currentMove, threatMove))
        {
          assert(-ScoreInfinite < beta - 1 && beta - 1 < ScoreInfinite);
          return beta - 1;
        }
      }
    }

    // step9
    // probcut
    if (!PVNode
      && SEARCH_PROBCUT_DEPTH_THRESHOLD <= depth
      && !ss->skipNullMove
      // 確実にバグらせないようにする。
      && abs(beta) < ScoreInfinite - 300)
    {
      const Score rbeta = beta + SEARCH_PROBCUT_RBETA_SCORE_DELTA;
      Depth rdepth = clamp(
        depth - SEARCH_PROBCUT_RBETA_DEPTH_DELTA, OnePly, depth - Depth1);

      assert(OnePly <= rdepth);
      assert(!(ss - 1)->currentMove.isNone());
      assert((ss - 1)->currentMove != Move::moveNull());

      // move.cap() は前回(一手前)の指し手で取った駒の種類
      MovePicker mp(pos, ttMove, thisThread->history, move.cap());
      const CheckInfo ci(pos);
      while (!(move = mp.nextMove()).isNone()) {
        if (pos.pseudoLegalMoveIsLegal<false, false, false>(move, ci.pinned)) {
          ss->currentMove = move;
          pos.doMove(move, st, ci, pos.moveGivesCheck(move, ci));
          (ss + 1)->staticEvalRaw.p[0][0] = ScoreNotEvaluated;
          assert(Depth0 < rdepth);
          score = -search<NonPV>(pos, ss + 1, -rbeta, -rbeta + 1, rdepth, !cutNode);
          pos.undoMove(move);
          if (rbeta <= score) {
            assert(-ScoreInfinite < score && score < ScoreInfinite);
            return score;
          }
        }
      }
    }

  iid_start:
    // step10
    // internal iterative deepening
    if ((PVNode ? SEARCH_INTERNAL_ITERATIVE_DEEPENING_PV_NODE_DEPTH_THRESHOLD
      : SEARCH_INTERNAL_ITERATIVE_DEEPENING_NON_PV_NODE_DEPTH_THRESHOLD) <= depth
      && ttMove.isNone()
      && (PVNode || (!inCheck && beta <= ss->staticEval
        + static_cast<Score>(SEARCH_INTERNAL_ITERATIVE_DEEPENING_SCORE_MARGIN))))
    {
      Depth d = PVNode
        ? (depth - SEARCH_INTERNAL_ITERATIVE_DEEPENING_PV_NODE_DEPTH_DELTA)
        : (depth * SEARCH_INTERNAL_ITERATIVE_DEEPENING_NON_PV_DEPTH_SCALE / FLOAT_SCALE);
      d = clamp(d, Depth1, depth - Depth1);

      ss->skipNullMove = true;
      assert(Depth0 < d);
      search<PVNode ? PV : NonPV>(pos, ss, alpha, beta, d, true);
      ss->skipNullMove = false;

      tte = TT.probe(posKey, ttHit);
      ttMove = (ttHit ?
        move16toMove(tte->move(), pos) :
        Move::moveNone());
    }

    MovePicker mp(pos, ttMove, depth, thisThread->history, ss, PVNode ? -ScoreInfinite : beta);
    const CheckInfo ci(pos);
    score = bestScore;
    singularExtensionNode =
      !RootNode
      && SEARCH_SINGULAR_EXTENSION_DEPTH_THRESHOLD <= depth
      && !ttMove.isNone()
      && excludedMove.isNone()
      && (tte->bound() & BoundLower)
      && depth - SEARCH_SINGULAR_EXTENSION_TTE_DEPTH_THRESHOLD <= tte->depth();

    // step11
    // Loop through moves
    while (!(move = mp.nextMove()).isNone()) {
      if (move == excludedMove) {
        continue;
      }

      if (RootNode
        && std::find(thisThread->rootMoves.begin() + thisThread->PVIdx,
          thisThread->rootMoves.end(),
          move) == thisThread->rootMoves.end())
      {
        continue;
      }

      ss->moveCount = ++moveCount;

      if (RootNode) {
        //Signals.firstRootMove = (moveCount == 1);
#if 0
        if (thisThread == threads.mainThread() && 3000 < searchTimer.elapsed()) {
          SYNCCOUT << "info depth " << depth / OnePly
            << " currmove " << move.toUSI()
            << " currmovenumber " << moveCount + pvIdx << SYNCENDL;
        }
#endif
      }

      if (PVNode)
        (ss + 1)->pv = nullptr;

      extension = Depth0;
      captureOrPawnPromotion = move.isCaptureOrPawnPromotion();
      givesCheck = pos.moveGivesCheck(move, ci);
      dangerous = givesCheck; // todo: not implement

      // step12
      if (givesCheck && ScoreZero <= pos.seeSign(move))
      {
        extension = OnePly;
      }

      // singuler extension
      if (singularExtensionNode
        && extension == Depth0
        && move == ttMove
        && pos.pseudoLegalMoveIsLegal<false, false, false>(move, ci.pinned)
        && abs(ttScore) < ScoreKnownWin)
      {
        assert(ttScore != ScoreNone);

        const Score rBeta = ttScore - static_cast<Score>(depth);
        ss->excludedMove = move;
        ss->skipNullMove = true;
        Depth nextDepth = SEARCH_SINGULAR_EXTENSION_NULL_WINDOW_SEARCH_DEPTH_SCALE * depth / FLOAT_SCALE;
        nextDepth = clamp(nextDepth, Depth1, depth - Depth1);
        assert(Depth0 < nextDepth);
        score = search<NonPV>(pos, ss, rBeta - 1, rBeta, nextDepth, cutNode);
        ss->skipNullMove = false;
        ss->excludedMove = Move::moveNone();

        if (score < rBeta) {
          //extension = OnePly;
          extension = (beta <= rBeta ? OnePly + OnePly / 2 : OnePly);
        }
      }

      newDepth = depth - OnePly + extension;

      // step13
      // futility pruning
      if (!PVNode
        && !captureOrPawnPromotion
        && !inCheck
        && !dangerous
        //&& move != ttMove // 次の行がtrueならこれもtrueなので条件から省く。
        && ScoreMatedInMaxPly < bestScore)
      {
        assert(move != ttMove);
        // move count based pruning
        // FutilityMoveCountsのサイズが16 * OnePlyなのでdepthの閾値は調整しない
        if (depth < 16 * OnePly
          && FutilityMoveCounts[depth] <= moveCount
          && (threatMove.isNone() || !refutes(pos, move, threatMove)))
        {
          continue;
        }

        // score based pruning
        const Depth predictedDepth = newDepth - reduction<PVNode>(depth, moveCount);
        // gain を 2倍にする。
        const Score futilityScore = ss->staticEval + futilityMargin(predictedDepth, moveCount)
          + SEARCH_FUTILITY_PRUNING_SCORE_GAIN_SLOPE * thisThread->gains.value(move.isDrop(), colorAndPieceTypeToPiece(pos.turn(), move.pieceTypeFromOrDropped()), move.to()) / FLOAT_SCALE;

        if (futilityScore < beta) {
          bestScore = std::max(bestScore, futilityScore);
          continue;
        }

        if (predictedDepth < SEARCH_FUTILITY_PRUNING_PREDICTED_DEPTH_THRESHOLD
          && pos.seeSign(move) < ScoreZero)
        {
          continue;
        }
      }

      // RootNode, SPNode はすでに合法手であることを確認済み。
      if (!RootNode && !pos.pseudoLegalMoveIsLegal<false, false, false>(move, ci.pinned)) {
        ss->moveCount = --moveCount;
        continue;
      }

      ss->currentMove = move;
      if (!captureOrPawnPromotion && playedMoveCount < 64) {
        movesSearched[playedMoveCount++] = move;
      }

      // 相手王を取って手駒にしてしまうバグに対するハック
      if (move.cap() == King) {
        //SYNCCOUT << "info string Searcher::search() Tried to capture the opponent's king." << SYNCENDL;
        return mateIn(ss->ply);
      }

      // step14
      pos.doMove(move, st, ci, givesCheck);
      (ss + 1)->staticEvalRaw.p[0][0] = ScoreNotEvaluated;

      // step15
      // LMR
      if (SEARCH_LATE_MOVE_REDUCTION_DEPTH_THRESHOLD <= depth
        && moveCount > 1
        && !captureOrPawnPromotion
        && move != ttMove
        && ss->killers[0] != move
        && ss->killers[1] != move)
      {
        ss->reduction = reduction<PVNode>(depth, moveCount);
        if (!PVNode && cutNode) {
          ss->reduction += OnePly;
        }
        Depth d = clamp(newDepth - ss->reduction, Depth1, depth - Depth1);
        // PVS
        assert(Depth0 < d);
        score = -search<NonPV>(pos, ss + 1, -(alpha + 1), -alpha, d, true);

        doFullDepthSearch = (alpha < score && ss->reduction != Depth0);
        ss->reduction = Depth0;
      }
      else {
        doFullDepthSearch = !PVNode || moveCount > 1;
      }

      // step16
      // full depth search
      // PVS
      if (doFullDepthSearch) {
        if (newDepth < OnePly) {
          if (givesCheck) {
            score = -qsearch<NonPV, true>(pos, ss + 1, -(alpha + 1), -alpha, Depth0);
          }
          else {
            score = -qsearch<NonPV, false>(pos, ss + 1, -(alpha + 1), -alpha, Depth0);
          }
        }
        else {
          assert(Depth0 < newDepth);
          score = -search<NonPV>(pos, ss + 1, -(alpha + 1), -alpha, newDepth, !cutNode);
        }
      }

      // 通常の探索
      if (PVNode && (moveCount == 1 || (alpha < score && (RootNode || score < beta)))) {
        (ss + 1)->pv = pv;
        (ss + 1)->pv[0] = Move::moveNone();

        if (newDepth < OnePly) {
          if (givesCheck) {
            score = -qsearch<PV, true>(pos, ss + 1, -beta, -alpha, Depth0);
          }
          else {
            score = -qsearch<PV, false>(pos, ss + 1, -beta, -alpha, Depth0);
          }
        }
        else {
          assert(Depth0 < newDepth);
          score = -search<PV>(pos, ss + 1, -beta, -alpha, newDepth, false);
        }
      }

      // step17
      pos.undoMove(move);

      assert(-ScoreInfinite < score && score < ScoreInfinite);

      // step18
      if (Signals.stop.load(std::memory_order_relaxed)) {
        return ScoreZero;
      }

      if (RootNode) {
        RootMove& rm = *std::find(thisThread->rootMoves.begin(), thisThread->rootMoves.end(), move);
        if (moveCount == 1 || alpha < score) {
          // PV move or new best move
          rm.score = score;
          rm.pv.resize(1);

          assert((ss + 1)->pv);

          for (Move* m = (ss + 1)->pv; *m != Move::moveNone(); ++m)
            rm.pv.push_back(*m);

          // We record how often the best move has been changed in each
          // iteration. This information is used for time management: When
          // the best move changes frequently, we allocate some more time.
          if (moveCount > 1 && thisThread == Threads.main())
            ++static_cast<MainThread*>(thisThread)->bestMoveChanges;

#if defined BISHOP_IN_DANGER
          if ((bishopInDangerFlag == BlackBishopInDangerIn28 && move.toCSA() == "0082KA")
            || (bishopInDangerFlag == WhiteBishopInDangerIn28 && move.toCSA() == "0028KA")
            || (bishopInDangerFlag == BlackBishopInDangerIn78 && move.toCSA() == "0032KA")
            || (bishopInDangerFlag == WhiteBishopInDangerIn78 && move.toCSA() == "0078KA"))
          {
            rm.score_ -= options[OptionNames::DANGER_DEMERIT_SCORE];
          }
#endif
          //rm.extract_ponder_from_tt(pos);

          //if (!isPVMove) {
          //  ++bestMoveChanges;
          //}
        }
        else {
          rm.score = -ScoreInfinite;
        }
      }

      if (bestScore < score) {
        bestScore = score;

        if (alpha < score) {
          // If there is an easy move for this position, clear it if unstable
          if (PVNode
            &&  thisThread == Threads.main()
            && EasyMove.get(pos.getKey()) != Move::moveNone()
            && (move != EasyMove.get(pos.getKey()) || moveCount > 1))
            EasyMove.clear();

          bestMove = move;

          if (PVNode && !RootNode) // Update pv even in fail-high case
            update_pv(ss->pv, move, (ss + 1)->pv);

          if (PVNode && score < beta) {
            alpha = score;
          }
          else {
            assert(score >= beta); // Fail high
            break;
          }
        }
      }
    }

    // step20
    if (moveCount == 0) {
      return !excludedMove.isNone() ? alpha : matedIn(ss->ply);
    }

    if (bestScore == -ScoreInfinite) {
      assert(playedMoveCount == 0);
      bestScore = alpha;
    }

    if (beta <= bestScore) {
      // failed high
      Score newTtScore = score_to_tt(bestScore, ss->ply);
      assert(-ScoreInfinite < newTtScore && newTtScore < ScoreInfinite);
      tte->save(posKey, newTtScore, BoundLower, depth,
        bestMove, ss->staticEval, TT.generation());

      if (!bestMove.isCaptureOrPawnPromotion() && !inCheck) {
        if (bestMove != ss->killers[0]) {
          ss->killers[1] = ss->killers[0];
          ss->killers[0] = bestMove;
        }

        const Score bonus = static_cast<Score>(depth * depth);
        const Piece pc1 = colorAndPieceTypeToPiece(pos.turn(), bestMove.pieceTypeFromOrDropped());
        thisThread->history.update(bestMove.isDrop(), pc1, bestMove.to(), bonus);

        for (int i = 0; i < playedMoveCount - 1; ++i) {
          const Move m = movesSearched[i];
          const Piece pc2 = colorAndPieceTypeToPiece(pos.turn(), m.pieceTypeFromOrDropped());
          thisThread->history.update(m.isDrop(), pc2, m.to(), -bonus);
        }
      }
    }
    else {
      // failed low or PV search
      Score newTtSearch = score_to_tt(bestScore, ss->ply);
      if (!(-ScoreInfinite < newTtSearch && newTtSearch < ScoreInfinite)) {
        pos.print();
        std::cerr << __FILE__ << " " << __FUNCTION__ << " " << __LINE__
          << " newTtSearch=" << newTtSearch
          << " bestScore=" << bestScore
          << " ss->ply=" << ss->ply
          << std::endl;
        assert(false);
      }
      //assert((-ScoreInfinite < newTtSearch && newTtSearch < ScoreInfinite));
      tte->save(posKey, newTtSearch,
        ((PVNode && !bestMove.isNone()) ? BoundExact : BoundUpper),
        depth, bestMove, ss->staticEval, TT.generation());
    }

    assert(-ScoreInfinite < bestScore && bestScore < ScoreInfinite);

    return bestScore;
  }


  // qsearch() is the quiescence search function, which is called by the main
  // search function when the remaining depth is zero (or, to be more precise,
  // less than ONE_PLY).

  template <NodeType NT, bool InCheck>
  Score qsearch(Position& pos, Search::SearchStack* ss, Score alpha, Score beta, Depth depth) {

    constexpr bool PVNode = (NT == PV);

    assert(NT == PV || NT == NonPV);
    assert(InCheck == pos.inCheck());
    assert(-ScoreInfinite <= alpha && alpha < beta && beta <= ScoreInfinite);
    assert(PVNode || (alpha == beta - 1));
    assert(depth <= Depth0);

    Move pv[MaxPly + 1];
    StateInfo st;
    TTEntry* tte;
    Key posKey;
    Move ttMove;
    Move move;
    Move bestMove;
    Score bestScore;
    Score score;
    Score ttScore;
    Score futilityScore;
    Score futilityBase;
    Score oldAlpha;
    bool givesCheck;
    bool evasionPrunable;
    Depth ttDepth;
    bool ttHit;

    if (PVNode) {
      oldAlpha = alpha;
      (ss + 1)->pv = pv;
      ss->pv[0] = Move::moveNone();
    }

    ss->currentMove = bestMove = Move::moveNone();
    ss->ply = (ss - 1)->ply + 1;

    if (MaxPly < ss->ply) {
      return ScoreDraw;
    }

    ttDepth = ((InCheck || DepthQChecks <= depth) ? DepthQChecks : DepthQNoChecks);

    posKey = pos.getKey();
    tte = TT.probe(posKey, ttHit);
    ttMove = (ttHit ? move16toMove(tte->move(), pos) : Move::moveNone());
    ttScore = (ttHit ? score_from_tt(tte->score(), ss->ply) : ScoreNone);
    if (!((-ScoreInfinite < ttScore && ttScore < ScoreInfinite) || ttScore == ScoreNone)) {
      pos.print();
      std::cerr << __FILE__ << " " << __FUNCTION__ << " " << __LINE__
        << " ttScore=" << ttScore
        << " tte->score()=" << tte->score()
        << " ss->ply=" << ss->ply
        << std::endl;
      assert(false);
    }

    // nonPVでは置換表の指し手で枝刈りする
    // PVでは置換表の指し手では枝刈りしない(前回evaluateした値は使える)
    if (!PVNode
      && ttHit
      && tte->depth() >= ttDepth
      && ttScore != ScoreNone // 置換表から取り出したときに他スレッドが値を潰している可能性があるのでこのチェックが必要
      && (ttScore >= beta ? (tte->bound() & BoundLower)
        : (tte->bound() & BoundUpper)))
      // ttValueが下界(真の評価値はこれより大きい)もしくはジャストな値で、かつttValue >= beta超えならbeta cutされる
      // ttValueが上界(真の評価値はこれより小さい)だが、tte->depth()のほうがdepthより深いということは、
      // 今回の探索よりたくさん探索した結果のはずなので、今回よりは枝刈りが甘いはずだから、その値を信頼して
      // このままこの値でreturnして良い。
    {
      ss->currentMove = ttMove;
      assert(-ScoreInfinite < ttScore && ttScore < ScoreInfinite);
      return ttScore;
    }

    pos.setNodesSearched(pos.nodesSearched() + 1);

    // 宣言勝ち
    {
      // 王手がかかってようがかかってまいが、宣言勝ちの判定は正しい。
      // (トライルールのとき王手を回避しながら入玉することはありうるので)
      bool nyugyokuWin = nyugyoku(pos);
      if (nyugyokuWin)
      {
        bestScore = mateIn(ss->ply + 1); // 1手詰めなのでこの次のnodeで(指し手がなくなって)詰むという解釈
        tte->save(posKey, score_to_tt(bestScore, ss->ply), BoundExact,
          DepthMax, Move::moveNone(), ss->staticEval, TT.generation());
        return bestScore;
      }
    }

    if (InCheck) {
      ss->staticEval = ScoreNone;
      bestScore = futilityBase = -ScoreInfinite;
    }
    else {
      if (!(move = pos.mateMoveIn1Ply()).isNone()) {
        score = mateIn(ss->ply);
        assert(-ScoreInfinite < score && score < ScoreInfinite);
        return score;
      }

      if (ttHit) {
        if ((ss->staticEval = bestScore = tte->eval()) == ScoreNone) {
          ss->staticEval = bestScore = evaluate(pos, ss);
        }
      }
      else {
        ss->staticEval = bestScore = evaluate(pos, ss);
      }

      if (beta <= bestScore) {
        if (!ttHit) {
          Score newTtScore = score_to_tt(bestScore, ss->ply);
          assert(-ScoreInfinite < newTtScore && newTtScore < ScoreInfinite);
          tte->save(pos.getKey(), score_to_tt(bestScore, ss->ply), BoundLower,
            DepthNone, Move::moveNone(), ss->staticEval, TT.generation());
        }

        assert(-ScoreInfinite < bestScore && bestScore < ScoreInfinite);
        return bestScore;
      }

      if (PVNode && alpha < bestScore) {
        alpha = bestScore;
      }

      futilityBase = bestScore + QSEARCH_FUTILITY_MARGIN;
    }

    evaluate(pos, ss);

    MovePicker mp(pos, ttMove, depth, pos.thisThread()->history, (ss - 1)->currentMove.to());
    const CheckInfo ci(pos);

    while (!(move = mp.nextMove()).isNone())
    {
      assert(pos.isOK());

      givesCheck = pos.moveGivesCheck(move, ci);

      // futility pruning
      if (!PVNode
        && !InCheck // 駒打ちは王手回避のみなので、ここで弾かれる。
        && !givesCheck
        && move != ttMove)
      {
        futilityScore =
          futilityBase + Position::capturePieceScore(pos.piece(move.to()));
        if (move.isPromotion()) {
          futilityScore += Position::promotePieceScore(move.pieceTypeFrom());
        }

        if (futilityScore < beta) {
          bestScore = std::max(bestScore, futilityScore);
          assert(-ScoreInfinite < bestScore && bestScore < ScoreInfinite);
          continue;
        }

        // todo: MovePicker のオーダリングで SEE してるので、ここで SEE するの勿体無い。
        if (futilityBase < beta
          && depth < Depth0
          && pos.see(move, beta - futilityBase) <= ScoreZero)
        {
          bestScore = std::max(bestScore, futilityBase);
          assert(-ScoreInfinite < bestScore && bestScore < ScoreInfinite);
          continue;
        }
      }

      evasionPrunable = (InCheck
        && ScoreMatedInMaxPly < bestScore
        && !move.isCaptureOrPawnPromotion());

      if (!PVNode
        && (!InCheck || evasionPrunable)
        && move != ttMove
        && (!move.isPromotion() || move.pieceTypeFrom() != Pawn)
        && pos.seeSign(move) < 0)
      {
        continue;
      }

      if (!pos.pseudoLegalMoveIsLegal<false, false, false>(move, ci.pinned)) {
        continue;
      }

      ss->currentMove = move;

      // 王を手駒に加えようとして落ちるバグに対するハック
      if (move.cap() == King) {
        //SYNCCOUT << "info string Searcher::qsearch() Tried to capture the opponent's king." << SYNCENDL;
        // TODO(nodchip): 置換表に保存しなくていよいのか？
        // 上にあるmateMoveIn1Ply()では保存していない。
        score = mateIn(ss->ply);
        assert(-ScoreInfinite < score && score < ScoreInfinite);
        return score;
      }

      pos.doMove(move, st, ci, givesCheck);
      (ss + 1)->staticEvalRaw.p[0][0] = ScoreNotEvaluated;
      score = (givesCheck ? -qsearch<NT, true>(pos, ss + 1, -beta, -alpha, depth - OnePly)
        : -qsearch<NT, false>(pos, ss + 1, -beta, -alpha, depth - OnePly));
      pos.undoMove(move);

      assert(-ScoreInfinite < score && score < ScoreInfinite);

      if (bestScore < score) {
        bestScore = score;

        if (alpha < score) {
          if (PVNode) // Update pv even in fail-high case
            update_pv(ss->pv, move, (ss + 1)->pv);

          if (PVNode && score < beta) {
            alpha = score;
            bestMove = move;
          }
          else {
            // fail high
            Score newTtScore = score_to_tt(score, ss->ply);
            assert(-ScoreInfinite < newTtScore && newTtScore < ScoreInfinite);
            tte->save(posKey, newTtScore, BoundLower,
              ttDepth, move, ss->staticEval, TT.generation());
            assert(-ScoreInfinite < score && score < ScoreInfinite);
            return score;
          }
        }
      }
    }

    if (InCheck && bestScore == -ScoreInfinite) {
      score = matedIn(ss->ply);
      assert(-ScoreInfinite < score && score < ScoreInfinite);
      return score;
    }

    Score newTtScore = score_to_tt(bestScore, ss->ply);
    assert(-ScoreInfinite < newTtScore && newTtScore < ScoreInfinite);
    tte->save(posKey, newTtScore,
      ((PVNode && oldAlpha < bestScore) ? BoundExact : BoundUpper),
      ttDepth, bestMove, ss->staticEval, TT.generation());

    assert(-ScoreInfinite < bestScore && bestScore < ScoreInfinite);

    return bestScore;
  }


  // value_to_tt() adjusts a mate score from "plies to mate from the root" to
  // "plies to mate from the current position". Non-mate scores are unchanged.
  // The function is called before storing a value in the transposition table.

  Score score_to_tt(const Score s, const Ply ply) {
    if (isMate(s)) {
      // 詰み
      return ScoreMate0Ply;
    }
    else if (isMated(s)) {
      // 詰まされている
      return ScoreMated0Ply;
    }
    else if (isSuperior(s)) {
      // 優等局面
      return ScoreSuperior0Ply;
    }
    else if (isInferior(s)) {
      // 劣等局面
      return ScoreInferior0Ply;
    }
    else if (abs(s) < ScoreSuperiorMaxPly) {
      return s;
    }

    std::cerr << __FILE__ << " " << __FUNCTION__ << " " << __LINE__
      << " s=" << s
      << " ply=" << ply
      << std::endl;
    assert(false);
    return ScoreNone;
  }

  // value_from_tt() is the inverse of value_to_tt(): It adjusts a mate score
  // from the transposition table (which refers to the plies to mate/be mated
  // from current position) to "plies to mate/be mated from the root".

  Score score_from_tt(const Score s, const Ply ply) {
    assert(0 <= ply);

    if (s == ScoreNone) {
      return ScoreNone;
    }
    else if (s == ScoreMate0Ply) {
      // 詰み
      return mateIn(ply);
    }
    else if (s == ScoreMated0Ply) {
      // 詰まされている
      return matedIn(ply);
    }
    else if (s == ScoreSuperior0Ply) {
      // 優等局面
      return superiorIn(ply);
    }
    else if (s == ScoreInferior0Ply) {
      // 劣等局面
      return inferiorIn(ply);
    }
    else if (abs(s) < ScoreSuperiorMaxPly) {
      return s;
    }

    std::cerr << __FILE__ << " " << __FUNCTION__ << " " << __LINE__
      << " s=" << s
      << " ply=" << ply
      << std::endl;
    assert(false);
    return ScoreNone;
  }

  // update_pv() adds current move and appends child pv[]

  void update_pv(Move* pv, Move move, Move* childPv) {

    for (*pv++ = move; childPv && *childPv != Move::moveNone(); )
      *pv++ = *childPv++;
    *pv = Move::moveNone();
  }

  // check_time() is used to print debug info and, more importantly, to detect
  // when we are out of available time and thus stop the search.

  void check_time() {

    static TimePoint lastInfoTime = now();

    int elapsed = Time.elapsed();
    TimePoint tick = Limits.startTime + elapsed;

    if (tick - lastInfoTime >= 1000)
    {
      lastInfoTime = tick;
      //dbg_print();
    }

    // An engine may not stop pondering until told so by the GUI
    if (Limits.ponder)
      return;

    if ((Limits.use_time_management() && elapsed > Time.maximum() - 10)
      || (Limits.movetime && elapsed >= Limits.movetime)
      || (Limits.nodes && Threads.nodes_searched() >= Limits.nodes))
      Signals.stop = true;
  }

  // 入玉勝ちかどうかを判定
  bool nyugyoku(const Position& pos) {
    // CSA ルールでは、一 から 六 の条件を全て満たすとき、入玉勝ち宣言が出来る。

    // 一 宣言側の手番である。

    // この関数を呼び出すのは自分の手番のみとする。ponder では呼び出さない。

    const Color us = pos.turn();
    // 敵陣のマスク
    const Bitboard opponentsField = (us == Black ? inFrontMask<Black, Rank6>() : inFrontMask<White, Rank4>());

    // 二 宣言側の玉が敵陣三段目以内に入っている。
    if (!pos.bbOf(King, us).andIsNot0(opponentsField))
      return false;

    // 三 宣言側が、大駒5点小駒1点で計算して
    //     先手の場合28点以上の持点がある。
    //     後手の場合27点以上の持点がある。
    //     点数の対象となるのは、宣言側の持駒と敵陣三段目以内に存在する玉を除く宣言側の駒のみである。
    const Bitboard bigBB = pos.bbOf(Rook, Dragon, Bishop, Horse) & opponentsField & pos.bbOf(us);
    const Bitboard smallBB = (pos.bbOf(Pawn, Lance, Knight, Silver) | pos.goldsBB()) & opponentsField & pos.bbOf(us);
    const Hand hand = pos.hand(us);
    const int val = (bigBB.popCount() + hand.numOf<HRook>() + hand.numOf<HBishop>()) * 5
      + smallBB.popCount()
      + hand.numOf<HPawn>() + hand.numOf<HLance>() + hand.numOf<HKnight>()
      + hand.numOf<HSilver>() + hand.numOf<HGold>();
#if defined LAW_24
    if (val < 31)
      return false;
#else
    if (val < (us == Black ? 28 : 27))
      return false;
#endif

    // 四 宣言側の敵陣三段目以内の駒は、玉を除いて10枚以上存在する。

    // 玉は敵陣にいるので、自駒が敵陣に11枚以上あればよい。
    if ((pos.bbOf(us) & opponentsField).popCount() < 11)
      return false;

    // 五 宣言側の玉に王手がかかっていない。
    if (pos.inCheck())
      return false;

    // 六 宣言側の持ち時間が残っている。

    // 持ち時間が無ければ既に負けなので、何もチェックしない。

    return true;
  }
} // namespace


  /// USI::pv() formats PV information according to the UCI protocol. UCI requires
  /// that all (if any) unsearched PV lines are sent using a previous search score.

std::string USI::pv(const Position& pos, Depth depth, Score alpha, Score beta) {
  std::stringstream ss;
  int elapsed = Time.elapsed() + 1;
  const Search::RootMoveVector& rootMoves = pos.thisThread()->rootMoves;
  size_t PVIdx = pos.thisThread()->PVIdx;
  size_t multiPV = std::min((size_t)Options[USI::OptionNames::MULTIPV], rootMoves.size());
  uint64_t nodes_searched = Threads.nodes_searched();

  for (size_t i = 0; i < multiPV; ++i) {
    bool updated = (i <= PVIdx);

    if (depth == 1 && !updated) {
      continue;
    }

    Ply d = updated ? depth : depth - 1;
    Score s = updated ? rootMoves[i].score : rootMoves[i].previousScore;

    ss << "info depth " << d
      << " seldepth " << pos.thisThread()->maxPly
      << " score " << (i == PVIdx ? USI::score(s, alpha, beta) : USI::score(s))
      << " nodes " << Threads.nodes_searched()
      << " nps " << Threads.nodes_searched() * 1000 / elapsed
      << " time " << elapsed
      << " multipv " << i + 1
#if defined(OUTPUT_TRANSPOSITION_TABLE_UTILIZATION)
      << " hashfull " << tt.getUtilizationPerMill()
#elif defined(OUTPUT_EVALUATE_HASH_TABLE_UTILIZATION)
      << " hashfull " << g_evalTable.getUtilizationPerMill()
#elif defined(OUTPUT_TRANSPOSITION_HIT_RATE)
      << " hashfull " << tt.getHitRatePerMill()
#elif defined(OUTPUT_EVALUATE_HASH_HIT_RATE)
      << " hashfull " << Evaluater::getHitRatePerMill()
#elif defined(OUTPUT_TRANSPOSITION_EXPIRATION_RATE)
      << " hashfull " << tt.getCacheExpirationRatePerMill()
#elif defined(OUTPUT_EVALUATE_HASH_EXPIRATION_RATE)
      << " hashfull " << Evaluater::getExpirationRatePerMill()
#endif
      << " pv ";

    for (Move m : rootMoves[i].pv)
      ss << " " << m.toUSI();

    if (i) {
      ss << std::endl;
    }
  }

#ifdef OUTPUT_TRANSPOSITION_EXPIRATION_RATE
  ss << std::endl
    << "info string numSaves=" << tt.getNumberOfSaves()
    << " numExpirations=" << tt.getNumberOfCacheExpirations() << std::endl;
#endif
  return ss.str();
}


/// RootMove::insert_pv_in_tt() is called at the end of a search iteration, and
/// inserts the PV back into the TT. This makes sure the old PV moves are searched
/// first, even if the old TT entries have been overwritten.

void RootMove::insert_pv_in_tt(Position& pos) {

  StateInfo state[MaxPly], *st = state;
  bool ttHit;

  for (Move m : pv)
  {
    assert(MoveList<LegalAll>(pos).contains(m));

    TTEntry* tte = TT.probe(pos.getKey(), ttHit);

    if (!ttHit || tte->move() != m) // Don't overwrite correct entries
      tte->save(pos.getKey(), ScoreNone, BoundNone, DepthNone,
        m, ScoreNone, TT.generation());

    pos.doMove(m, *st++);
  }

  for (size_t i = pv.size(); i > 0; )
    pos.undoMove(pv[--i]);
}


/// RootMove::extract_ponder_from_tt() is called in case we have no ponder move
/// before exiting the search, for instance in case we stop the search during a
/// fail high at root. We try hard to have a ponder move to return to the GUI,
/// otherwise in case of 'ponder on' we have nothing to think on.

bool RootMove::extract_ponder_from_tt(Position& pos, Move ponderCandidate)
{
  StateInfo st;
  bool ttHit;

  assert(pv.size() == 1);

  if (pv[0] == Move::moveNone() || pv[0] == Move::moveNull()) {
    return false;
  }

  pos.doMove(pv[0], st);
  TTEntry* tte = TT.probe(pos.getKey(), ttHit);
  Move m;
  if (ttHit) {
    m = tte->move(); // SMP safeにするためlocal copy
    if (MoveList<LegalAll>(pos).contains(m)) {
      pos.undoMove(pv[0]);
      pv.push_back(m);
      return true;
    }
  }

  // 置換表にもなかったので以前のiteration時のpv[1]をほじくり返す。
  m = ponderCandidate;
  if (MoveList<LegalAll>(pos).contains(m)) {
    pos.undoMove(pv[0]);
    pv.push_back(m);
    return true;
  }

  pos.undoMove(pv[0]);
  return false;
}
