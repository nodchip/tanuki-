﻿#include "generateMoves.hpp"
#include "usi.hpp"

namespace generate_moves
{
  // 角, 飛車の場合
  template <MoveType MT, PieceType PT, Color US, bool ALL>
  FORCE_INLINE MoveStack* generateBishopOrRookMoves(MoveStack* moveStackList, const Position& pos,
    const Bitboard& target, const Square /*ksq*/)
  {
    Bitboard fromBB = pos.bbOf(PT, US);
    while (fromBB.isNot0()) {
      const Square from = fromBB.firstOneFromI9();
      const bool fromCanPromote = canPromote(US, makeRank(from));
      Bitboard toBB = pos.attacksFrom<PT>(US, from) & target;
      while (toBB.isNot0()) {
        const Square to = toBB.firstOneFromI9();
        const bool toCanPromote = canPromote(US, makeRank(to));
        if (fromCanPromote | toCanPromote) {
          (*moveStackList++).move = makePromoteMove<MT>(PT, from, to, pos);
          if (MT == NonEvasion || ALL)
            (*moveStackList++).move = makeNonPromoteMove<MT>(PT, from, to, pos);
        }
        else // 角、飛車は成れるなら成り、不成は生成しない。
          (*moveStackList++).move = makeNonPromoteMove<MT>(PT, from, to, pos);
      }
    }
    return moveStackList;
  }

  // 駒打ちの場合
  // 歩以外の持ち駒は、loop の前に持ち駒の種類の数によって switch で展開している。
  // ループの展開はコードが膨れ上がる事によるキャッシュヒット率の低下と、演算回数のバランスを取って決める必要がある。
  // NPSに影響が出ないならシンプルにした方が良さそう。
  template <Color US>
  MoveStack* generateDropMoves20151211(MoveStack* moveStackList, const Position& pos, const Bitboard& target) {
    const Hand hand = pos.hand(US);
    // まず、歩に対して指し手を生成
    if (hand.exists<HPawn>()) {
      Bitboard toBB = target;
      // 一段目には打てない
      const Rank TRank9 = (US == Black ? Rank9 : Rank1);
      toBB.andEqualNot(rankMask<TRank9>());

      // 二歩の回避
      Bitboard pawnsBB = pos.bbOf(Pawn, US);
      Square pawnsSquare;
      foreachBB(pawnsBB, pawnsSquare, [&](const int part) {
        toBB.set(part, toBB.p(part) & ~squareFileMask(pawnsSquare).p(part));
      });

      // 打ち歩詰めの回避
      const Rank TRank1 = (US == Black ? Rank1 : Rank9);
      const SquareDelta TDeltaS = (US == Black ? DeltaS : DeltaN);

      const Square ksq = pos.kingSquare(oppositeColor(US));
      // 相手玉が九段目なら、歩で王手出来ないので、打ち歩詰めを調べる必要はない。
      if (makeRank(ksq) != TRank1) {
        const Square pawnDropCheckSquare = ksq + TDeltaS;
        assert(isInSquare(pawnDropCheckSquare));
        if (toBB.isSet(pawnDropCheckSquare) && pos.piece(pawnDropCheckSquare) == Empty) {
          if (!pos.isPawnDropCheckMate(US, pawnDropCheckSquare)) {
            // ここで clearBit だけして MakeMove しないことも出来る。
            // 指し手が生成される順番が変わり、王手が先に生成されるが、後で問題にならないか?
            (*moveStackList++).move = makeDropMove(Pawn, pawnDropCheckSquare);
          }
          toBB.xorBit(pawnDropCheckSquare);
        }
      }

      Square to;
      FOREACH_BB(toBB, to, {
        (*moveStackList++).move = makeDropMove(Pawn, to);
      });
    }

    // 歩 以外の駒を持っているか
    if (hand.exceptPawnExists()) {
      PieceType haveHand[6]; // 歩以外の持ち駒。vector 使いたいけど、速度を求めるので使わない。
      int haveHandNum = 0; // 持ち駒の駒の種類の数

                           // 桂馬、香車、それ以外の順番で格納する。(駒を打てる位置が限定的な順)
      if (hand.exists<HKnight>()) { haveHand[haveHandNum++] = Knight; }
      const int noKnightIdx = haveHandNum; // 桂馬を除く駒でループするときのループの初期値
      if (hand.exists<HLance >()) { haveHand[haveHandNum++] = Lance; }
      const int noKnightLanceIdx = haveHandNum; // 桂馬, 香車を除く駒でループするときのループの初期値
      if (hand.exists<HSilver>()) { haveHand[haveHandNum++] = Silver; }
      if (hand.exists<HGold  >()) { haveHand[haveHandNum++] = Gold; }
      if (hand.exists<HBishop>()) { haveHand[haveHandNum++] = Bishop; }
      if (hand.exists<HRook  >()) { haveHand[haveHandNum++] = Rook; }

      const Rank TRank8 = (US == Black ? Rank8 : Rank2);
      const Rank TRank9 = (US == Black ? Rank9 : Rank1);
      const Bitboard TRank8BB = rankMask<TRank8>();
      const Bitboard TRank9BB = rankMask<TRank9>();

      Bitboard toBB;
      Square to;
      // 桂馬、香車 以外の持ち駒があれば、
      // 一段目に対して、桂馬、香車以外の指し手を生成。
      switch (haveHandNum - noKnightLanceIdx) {
      case 0: break; // 桂馬、香車 以外の持ち駒がない。
      case 1: toBB = target & TRank9BB; FOREACH_BB(toBB, to, { Unroller<1>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[noKnightLanceIdx + i], to); }); }); break;
      case 2: toBB = target & TRank9BB; FOREACH_BB(toBB, to, { Unroller<2>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[noKnightLanceIdx + i], to); }); }); break;
      case 3: toBB = target & TRank9BB; FOREACH_BB(toBB, to, { Unroller<3>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[noKnightLanceIdx + i], to); }); }); break;
      case 4: toBB = target & TRank9BB; FOREACH_BB(toBB, to, { Unroller<4>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[noKnightLanceIdx + i], to); }); }); break;
      default: UNREACHABLE;
      }

      // 桂馬以外の持ち駒があれば、
      // 二段目に対して、桂馬以外の指し手を生成。
      switch (haveHandNum - noKnightIdx) {
      case 0: break; // 桂馬 以外の持ち駒がない。
      case 1: toBB = target & TRank8BB; FOREACH_BB(toBB, to, { Unroller<1>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[noKnightIdx + i], to); }); }); break;
      case 2: toBB = target & TRank8BB; FOREACH_BB(toBB, to, { Unroller<2>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[noKnightIdx + i], to); }); }); break;
      case 3: toBB = target & TRank8BB; FOREACH_BB(toBB, to, { Unroller<3>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[noKnightIdx + i], to); }); }); break;
      case 4: toBB = target & TRank8BB; FOREACH_BB(toBB, to, { Unroller<4>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[noKnightIdx + i], to); }); }); break;
      case 5: toBB = target & TRank8BB; FOREACH_BB(toBB, to, { Unroller<5>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[noKnightIdx + i], to); }); }); break;
      default: UNREACHABLE;
      }

      // 一、二段目以外に対して、全ての持ち駒の指し手を生成。
      toBB = target & ~(TRank8BB | TRank9BB);
      switch (haveHandNum) {
      case 0: assert(false); break; // 最適化の為のダミー
      case 1: FOREACH_BB(toBB, to, { Unroller<1>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[i], to); }); }); break;
      case 2: FOREACH_BB(toBB, to, { Unroller<2>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[i], to); }); }); break;
      case 3: FOREACH_BB(toBB, to, { Unroller<3>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[i], to); }); }); break;
      case 4: FOREACH_BB(toBB, to, { Unroller<4>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[i], to); }); }); break;
      case 5: FOREACH_BB(toBB, to, { Unroller<5>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[i], to); }); }); break;
      case 6: FOREACH_BB(toBB, to, { Unroller<6>()([&](const int i) { (*moveStackList++).move = makeDropMove(haveHand[i], to); }); }); break;
      default: UNREACHABLE;
      }
    }

    return moveStackList;
  }

  using xmm = __m128i;

  // 歩以外の駒打ちを生成する。
  // target: 空いているマスを表すビットボード
  // haveHand: 駒打ちを生成する駒の種類の配列
  // numberOfHaveHand: 駒打ちを生成する駒の種類の数
  // moves: 生成した駒打ち
  // numberOfMoves: 生成した駒打ちの総数
  void generaateDropMovesNonPawn(
    Bitboard target,
    PieceType haveHands[8],
    int numberOfHaveHands,
    u16 moves[1024],
    int& numberOfMoves)
  {
    // toだけを埋めたMove (下位16ビット)
    alignas(32) u16 moveToTemplates[128];
    // toの数
    int numberOfTos = 0;
    Square to;
    FOREACH_BB(target, to, {
      moveToTemplates[numberOfTos++] = static_cast<u16>(to2Move(to).value());
    });
    for (int haveHandIndex = 0; haveHandIndex < numberOfHaveHands; ++haveHandIndex) {
      // PieceTypeだけを埋めたMove (下位16ビット)
      u16 movePieceTypeTemplate = static_cast<u16>(makeDropMove(haveHands[haveHandIndex], (Square)0).value());
      // PieceTypeだけを埋めたMove 16マス分
      // _mm256_set1_epi16()を使うと上位128ビットがクリアされてしまうため
      // このようにしている。
      // TODO(hnoda): 上位128ビットがクリアされる原因の調査
      xmm movePieceTypeTemplates = _mm_set1_epi16(movePieceTypeTemplate);
      // 16マス分ずつ生成する
      for (int i = 0; i < numberOfTos; i += 8) {
        xmm moveTo = _mm_load_si128((const xmm*)&moveToTemplates[i]);
        xmm moveMerged = _mm_or_si128(moveTo, movePieceTypeTemplates);
        _mm_storeu_si128((xmm*)&moves[numberOfMoves], moveMerged);
        numberOfMoves += std::min(numberOfTos - i, 8);
      }
    }
  }

  // 駒打ちの場合
  // 歩以外の持ち駒は、loop の前に持ち駒の種類の数によって switch で展開している。
  // ループの展開はコードが膨れ上がる事によるキャッシュヒット率の低下と、演算回数のバランスを取って決める必要がある。
  // NPSに影響が出ないならシンプルにした方が良さそう。
  template <Color US>
  MoveStack* generateDropMoves(MoveStack* moveStackList, const Position& pos, const Bitboard& target) {
    const Hand hand = pos.hand(US);
    // まず、歩に対して指し手を生成
    if (hand.exists<HPawn>()) {
      Bitboard toBB = target;
      // 一段目には打てない
      const Rank TRank9 = (US == Black ? Rank9 : Rank1);
      toBB.andEqualNot(rankMask<TRank9>());

      // 二歩の回避
      Bitboard pawnsBB = pos.bbOf(Pawn, US);
      Square pawnsSquare;
      foreachBB(pawnsBB, pawnsSquare, [&](const int part) {
        toBB.set(part, toBB.p(part) & ~squareFileMask(pawnsSquare).p(part));
      });

      // 打ち歩詰めの回避
      const Rank TRank1 = (US == Black ? Rank1 : Rank9);
      const SquareDelta TDeltaS = (US == Black ? DeltaS : DeltaN);

      const Square ksq = pos.kingSquare(oppositeColor(US));
      // 相手玉が九段目なら、歩で王手出来ないので、打ち歩詰めを調べる必要はない。
      if (makeRank(ksq) != TRank1) {
        const Square pawnDropCheckSquare = ksq + TDeltaS;
        assert(isInSquare(pawnDropCheckSquare));
        if (toBB.isSet(pawnDropCheckSquare) && pos.piece(pawnDropCheckSquare) == Empty) {
          if (!pos.isPawnDropCheckMate(US, pawnDropCheckSquare)) {
            // ここで clearBit だけして MakeMove しないことも出来る。
            // 指し手が生成される順番が変わり、王手が先に生成されるが、後で問題にならないか?
            (*moveStackList++).move = makeDropMove(Pawn, pawnDropCheckSquare);
          }
          toBB.xorBit(pawnDropCheckSquare);
        }
      }

      Square to;
      FOREACH_BB(toBB, to, {
        (*moveStackList++).move = makeDropMove(Pawn, to);
      });
    }

    // 歩 以外の駒を持っているか
    if (!hand.exceptPawnExists()) {
      return moveStackList;
    }

    const Rank TRank8 = (US == Black ? Rank8 : Rank2);
    const Rank TRank9 = (US == Black ? Rank9 : Rank1);
    Bitboard TRank8BB = rankMask<TRank8>();
    Bitboard TRank9BB = rankMask<TRank9>();
    Bitboard TRank17BB = target;
    TRank17BB.andEqualNot(TRank8BB);
    TRank17BB.andEqualNot(TRank9BB);

    alignas(32) u16 moves[1024];
    int numberOfMoves = 0;

    // 歩以外の持ち駒。vector 使いたいけど、速度を求めるので使わない。
    PieceType haveHands[8];
    // 持ち駒の駒の種類の数
    int numberOfHaveHands = 0;

    // 持ち駒の中で三～九段目に打てる駒を列挙する
    if (hand.exists<HRook  >()) { haveHands[numberOfHaveHands++] = Rook; }
    if (hand.exists<HBishop>()) { haveHands[numberOfHaveHands++] = Bishop; }
    if (hand.exists<HGold  >()) { haveHands[numberOfHaveHands++] = Gold; }
    if (hand.exists<HSilver>()) { haveHands[numberOfHaveHands++] = Silver; }
    if (hand.exists<HKnight>()) { haveHands[numberOfHaveHands++] = Knight; }
    if (hand.exists<HLance >()) { haveHands[numberOfHaveHands++] = Lance; }

    // 高速化のため歩以外の手駒の種類が1種類の場合は単純なループを使用する
    if (numberOfHaveHands == 1) {
      Bitboard toBB = target;
      if (haveHands[0] == Lance) {
        toBB.andEqualNot(TRank9BB);
      }
      else if (haveHands[0] == Knight) {
        toBB.andEqualNot(TRank9BB);
        toBB.andEqualNot(TRank8BB);
      }
      Square to;
      FOREACH_BB(toBB, to, { (*moveStackList++).move = makeDropMove(haveHands[0], to); });

      return moveStackList;
    }

    // 三～九段目の駒打ちを生成する
    generaateDropMovesNonPawn(
      Bitboard(target).andEqualNot(TRank8BB).andEqualNot(TRank9BB), haveHands, numberOfHaveHands, moves, numberOfMoves);

    // 持ち駒の中で二段目に打てる駒を列挙する
    numberOfHaveHands = 0;
    if (hand.exists<HRook  >()) { haveHands[numberOfHaveHands++] = Rook; }
    if (hand.exists<HBishop>()) { haveHands[numberOfHaveHands++] = Bishop; }
    if (hand.exists<HGold  >()) { haveHands[numberOfHaveHands++] = Gold; }
    if (hand.exists<HSilver>()) { haveHands[numberOfHaveHands++] = Silver; }
    if (hand.exists<HLance >()) { haveHands[numberOfHaveHands++] = Lance; }

    // 二段目の駒打ちを生成する
    generaateDropMovesNonPawn(
      target & TRank8BB, haveHands, numberOfHaveHands, moves, numberOfMoves);

    // 持ち駒の中で一段目に打てる駒を列挙する
    numberOfHaveHands = 0;
    if (hand.exists<HRook  >()) { haveHands[numberOfHaveHands++] = Rook; }
    if (hand.exists<HBishop>()) { haveHands[numberOfHaveHands++] = Bishop; }
    if (hand.exists<HGold  >()) { haveHands[numberOfHaveHands++] = Gold; }
    if (hand.exists<HSilver>()) { haveHands[numberOfHaveHands++] = Silver; }

    // 一段目の駒打ちを生成する
    generaateDropMovesNonPawn(
      target & TRank9BB, haveHands, numberOfHaveHands, moves, numberOfMoves);

    // 2手分ずつ16ビット->64ビットに変換して
    // moveStackListに格納していく
    for (int i = 0; i < numberOfMoves; ) {
      // 8手分ロードする
      xmm moves16 = _mm_load_si128((const xmm*)&moves[i]);
      int j = std::min(numberOfMoves, i + 8);
      for (; i < j; i += 2) {
        // 2手分ずつストアする
        xmm moves64 = _mm_cvtepi16_epi64(moves16);
        _mm_storeu_si128((xmm*)moveStackList, moves64);
        moveStackList += std::min(numberOfMoves - i, 2);
        moves16 = _mm_srli_si128(moves16, 4);
      }
    }

    return moveStackList;
  }

  // 金, 成り金、馬、竜の指し手生成
  template <MoveType MT, PieceType PT, Color US, bool ALL> struct GeneratePieceMoves {
    FORCE_INLINE MoveStack* operator () (MoveStack* moveStackList, const Position& pos, const Bitboard& target, const Square /*ksq*/) {
      static_assert(PT == GoldHorseDragon, "");
      // 金、成金、馬、竜のbitboardをまとめて扱う。
      Bitboard fromBB = (pos.goldsBB() | pos.bbOf(Horse, Dragon)) & pos.bbOf(US);
      while (fromBB.isNot0()) {
        const Square from = fromBB.firstOneFromI9();
        // from にある駒の種類を判別
        const PieceType pt = pieceToPieceType(pos.piece(from));
        Bitboard toBB = pos.attacksFrom(pt, US, from) & target;
        while (toBB.isNot0()) {
          const Square to = toBB.firstOneFromI9();
          (*moveStackList++).move = makeNonPromoteMove<MT>(pt, from, to, pos);
        }
      }
      return moveStackList;
    }
  };
  // 歩の場合
  template <MoveType MT, Color US, bool ALL> struct GeneratePieceMoves<MT, Pawn, US, ALL> {
    FORCE_INLINE MoveStack* operator () (MoveStack* moveStackList, const Position& pos, const Bitboard& target, const Square /*ksq*/) {
      // Txxx は先手、後手の情報を吸収した変数。数字は先手に合わせている。
      const Rank TRank6 = (US == Black ? Rank6 : Rank4);
      const Bitboard TRank789BB = inFrontMask<US, TRank6>();
      const SquareDelta TDeltaS = (US == Black ? DeltaS : DeltaN);

      Bitboard toBB = pawnAttack<US>(pos.bbOf(Pawn, US)) & target;

      // 成り
      if (MT != NonCaptureMinusPro) {
        Bitboard toOn789BB = toBB & TRank789BB;
        if (toOn789BB.isNot0()) {
          toBB.andEqualNot(TRank789BB);
          Square to;
          FOREACH_BB(toOn789BB, to, {
            const Square from = to + TDeltaS;
          (*moveStackList++).move = makePromoteMove<MT>(Pawn, from, to, pos);
          if (MT == NonEvasion || ALL) {
            const Rank TRank9 = (US == Black ? Rank9 : Rank1);
            if (makeRank(to) != TRank9) {
              (*moveStackList++).move = makeNonPromoteMove<MT>(Pawn, from, to, pos);
            }
          }
          });
        }
      }
      else {
        assert(!(target & TRank789BB).isNot0());
      }

      // 残り(不成)
      // toBB は 8~4 段目まで。
      Square to;
      FOREACH_BB(toBB, to, {
        const Square from = to + TDeltaS;
      (*moveStackList++).move = makeNonPromoteMove<MT>(Pawn, from, to, pos);
      });
      return moveStackList;
    }
  };
  // 香車の場合
  template <MoveType MT, Color US, bool ALL> struct GeneratePieceMoves<MT, Lance, US, ALL> {
    FORCE_INLINE MoveStack* operator () (MoveStack* moveStackList, const Position& pos, const Bitboard& target, const Square /*ksq*/) {
      Bitboard fromBB = pos.bbOf(Lance, US);
      while (fromBB.isNot0()) {
        const Square from = fromBB.firstOneFromI9();
        Bitboard toBB = pos.attacksFrom<Lance>(US, from) & target;
        do {
          if (toBB.isNot0()) {
            // 駒取り対象は必ず一つ以下なので、toBB のビットを 0 にする必要がない。
            const Square to = (MT == Capture || MT == CapturePlusPro ? toBB.constFirstOneFromI9() : toBB.firstOneFromI9());
            const bool toCanPromote = canPromote(US, makeRank(to));
            if (toCanPromote) {
              (*moveStackList++).move = makePromoteMove<MT>(Lance, from, to, pos);
              if (MT == NonEvasion || ALL) {
                if (isBehind<US, Rank9, Rank1>(makeRank(to))) // 1段目の不成は省く
                  (*moveStackList++).move = makeNonPromoteMove<MT>(Lance, from, to, pos);
              }
              else if (MT != NonCapture && MT != NonCaptureMinusPro) { // 駒を取らない3段目の不成を省く
                if (isBehind<US, Rank8, Rank2>(makeRank(to))) // 2段目の不成を省く
                  (*moveStackList++).move = makeNonPromoteMove<MT>(Lance, from, to, pos);
              }
            }
            else
              (*moveStackList++).move = makeNonPromoteMove<MT>(Lance, from, to, pos);
          }
          // 駒取り対象は必ず一つ以下なので、loop は不要。最適化で do while が無くなると良い。
        } while (!(MT == Capture || MT == CapturePlusPro) && toBB.isNot0());
      }
      return moveStackList;
    }
  };
  // 桂馬の場合
  template <MoveType MT, Color US, bool ALL> struct GeneratePieceMoves<MT, Knight, US, ALL> {
    FORCE_INLINE MoveStack* operator () (MoveStack* moveStackList, const Position& pos, const Bitboard& target, const Square /*ksq*/) {
      Bitboard fromBB = pos.bbOf(Knight, US);
      while (fromBB.isNot0()) {
        const Square from = fromBB.firstOneFromI9();
        Bitboard toBB = pos.attacksFrom<Knight>(US, from) & target;
        while (toBB.isNot0()) {
          const Square to = toBB.firstOneFromI9();
          const bool toCanPromote = canPromote(US, makeRank(to));
          if (toCanPromote) {
            (*moveStackList++).move = makePromoteMove<MT>(Knight, from, to, pos);
            if (isBehind<US, Rank8, Rank2>(makeRank(to))) // 1, 2段目の不成は省く
              (*moveStackList++).move = makeNonPromoteMove<MT>(Knight, from, to, pos);
          }
          else
            (*moveStackList++).move = makeNonPromoteMove<MT>(Knight, from, to, pos);
        }
      }
      return moveStackList;
    }
  };
  // 銀の場合
  template <MoveType MT, Color US, bool ALL> struct GeneratePieceMoves<MT, Silver, US, ALL> {
    FORCE_INLINE MoveStack* operator () (MoveStack* moveStackList, const Position& pos, const Bitboard& target, const Square /*ksq*/) {
      Bitboard fromBB = pos.bbOf(Silver, US);
      while (fromBB.isNot0()) {
        const Square from = fromBB.firstOneFromI9();
        const bool fromCanPromote = canPromote(US, makeRank(from));
        Bitboard toBB = pos.attacksFrom<Silver>(US, from) & target;
        while (toBB.isNot0()) {
          const Square to = toBB.firstOneFromI9();
          const bool toCanPromote = canPromote(US, makeRank(to));
          if (fromCanPromote | toCanPromote)
            (*moveStackList++).move = makePromoteMove<MT>(Silver, from, to, pos);
          (*moveStackList++).move = makeNonPromoteMove<MT>(Silver, from, to, pos);
        }
      }
      return moveStackList;
    }
  };
  template <MoveType MT, Color US, bool ALL> struct GeneratePieceMoves<MT, Bishop, US, ALL> {
    FORCE_INLINE MoveStack* operator () (MoveStack* moveStackList, const Position& pos, const Bitboard& target, const Square ksq) {
      return generateBishopOrRookMoves<MT, Bishop, US, ALL>(moveStackList, pos, target, ksq);
    }
  };
  template <MoveType MT, Color US, bool ALL> struct GeneratePieceMoves<MT, Rook, US, ALL> {
    FORCE_INLINE MoveStack* operator () (MoveStack* moveStackList, const Position& pos, const Bitboard& target, const Square ksq) {
      return generateBishopOrRookMoves<MT, Rook, US, ALL>(moveStackList, pos, target, ksq);
    }
  };
  // 玉の場合
  // 必ず盤上に 1 枚だけあることを前提にすることで、while ループを 1 つ無くして高速化している。
  template <MoveType MT, Color US, bool ALL> struct GeneratePieceMoves<MT, King, US, ALL> {
    FORCE_INLINE MoveStack* operator () (MoveStack* moveStackList, const Position& pos, const Bitboard& target, const Square /*ksq*/) {
      const Square from = pos.kingSquare(US);
      Bitboard toBB = pos.attacksFrom<King>(US, from) & target;
      while (toBB.isNot0()) {
        const Square to = toBB.firstOneFromI9();
        (*moveStackList++).move = makeNonPromoteMove<MT>(King, from, to, pos);
      }
      return moveStackList;
    }
  };

  // pin は省かない。
  FORCE_INLINE MoveStack* generateRecaptureMoves(MoveStack* moveStackList, const Position& pos, const Square to, const Color us) {
    Bitboard fromBB = pos.attackersTo(us, to);
    while (fromBB.isNot0()) {
      const Square from = fromBB.firstOneFromI9();
      const PieceType pt = pieceToPieceType(pos.piece(from));
      switch (pt) {
      case Empty: assert(false); break; // 最適化の為のダミー
      case Pawn: case Lance: case Knight: case Silver: case Bishop: case Rook:
        (*moveStackList++).move = ((canPromote(us, makeRank(to)) | canPromote(us, makeRank(from))) ?
          makePromoteMove<Capture>(pt, from, to, pos) :
          makeNonPromoteMove<Capture>(pt, from, to, pos));
        break;
      case Gold: case King: case ProPawn: case ProLance: case ProKnight: case ProSilver: case Horse: case Dragon:
        (*moveStackList++).move = makeNonPromoteMove<Capture>(pt, from, to, pos);
        break;
      default: UNREACHABLE;
      }
    }
    return moveStackList;
  }

  // 指し手生成 functor
  // テンプレート引数が複数あり、部分特殊化したかったので、関数ではなく、struct にした。
  // ALL == true のとき、歩、飛、角の不成、香の2段目の不成、香の3段目の駒を取らない不成も生成する。
  template <MoveType MT, Color US, bool ALL = false> struct GenerateMoves {
    MoveStack* operator () (MoveStack* moveStackList, const Position& pos) {
      static_assert(MT == Capture || MT == NonCapture || MT == CapturePlusPro || MT == NonCaptureMinusPro, "");
      // Txxx は先手、後手の情報を吸収した変数。数字は先手に合わせている。
      const Rank TRank6 = (US == Black ? Rank6 : Rank4);
      const Rank TRank7 = (US == Black ? Rank7 : Rank3);
      const Rank TRank8 = (US == Black ? Rank8 : Rank2);
      const Bitboard TRank789BB = inFrontMask<US, TRank6>();
      const Bitboard TRank1_6BB = inFrontMask<oppositeColor(US), TRank7>();
      const Bitboard TRank1_7BB = inFrontMask<oppositeColor(US), TRank8>();

      const Bitboard targetPawn =
        (MT == Capture) ? pos.bbOf(oppositeColor(US)) :
        (MT == NonCapture) ? pos.emptyBB() :
        (MT == CapturePlusPro) ? pos.bbOf(oppositeColor(US)) | (pos.occupiedBB().notThisAnd(TRank789BB)) :
        (MT == NonCaptureMinusPro) ? pos.occupiedBB().notThisAnd(TRank1_6BB) :
        allOneBB(); // error
      const Bitboard targetOther =
        (MT == Capture) ? pos.bbOf(oppositeColor(US)) :
        (MT == NonCapture) ? pos.emptyBB() :
        (MT == CapturePlusPro) ? pos.bbOf(oppositeColor(US)) :
        (MT == NonCaptureMinusPro) ? pos.emptyBB() :
        allOneBB(); // error
      const Square ksq = pos.kingSquare(oppositeColor(US));

      moveStackList = GeneratePieceMoves<MT, Pawn, US, ALL>()(moveStackList, pos, targetPawn, ksq);
      moveStackList = GeneratePieceMoves<MT, Lance, US, ALL>()(moveStackList, pos, targetOther, ksq);
      moveStackList = GeneratePieceMoves<MT, Knight, US, ALL>()(moveStackList, pos, targetOther, ksq);
      moveStackList = GeneratePieceMoves<MT, Silver, US, ALL>()(moveStackList, pos, targetOther, ksq);
      moveStackList = GeneratePieceMoves<MT, Bishop, US, ALL>()(moveStackList, pos, targetOther, ksq);
      moveStackList = GeneratePieceMoves<MT, Rook, US, ALL>()(moveStackList, pos, targetOther, ksq);
      moveStackList = GeneratePieceMoves<MT, GoldHorseDragon, US, ALL>()(moveStackList, pos, targetOther, ksq);
      moveStackList = GeneratePieceMoves<MT, King, US, ALL>()(moveStackList, pos, targetOther, ksq);

      return moveStackList;
    }
  };

  // 部分特殊化
  // 駒打ち生成
  template <Color US> struct GenerateMoves<Drop, US> {
    FORCE_INLINE MoveStack* operator () (MoveStack* moveStackList, const Position& pos) {
      const Bitboard target = pos.emptyBB();
      moveStackList = generateDropMoves<US>(moveStackList, pos, target);
      return moveStackList;
    }
  };

  // checkSq にある駒で王手されたとき、玉はその駒の利きの位置には移動できないので、移動できない位置を bannnedKingToBB に格納する。
  // 両王手のときには二度連続で呼ばれるため、= ではなく |= を使用している。
  // 最初に呼ばれたときは、bannedKingToBB == allZeroBB() である。
  // todo: FOECE_INLINE と template 省いてNPS比較
  template <Color THEM>
  FORCE_INLINE void makeBannedKingTo(Bitboard& bannedKingToBB, const Position& pos,
    const Square checkSq, const Square ksq)
  {
    switch (pos.piece(checkSq)) {
      //		case Empty: assert(false); break; // 最適化の為のダミー
    case (THEM == Black ? BPawn : WPawn) :
    case (THEM == Black ? BKnight : WKnight) :
      // 歩、桂馬で王手したときは、どこへ逃げても、その駒で取られることはない。
      // よって、ここでは何もしない。
      assert(
        pos.piece(checkSq) == (THEM == Black ? BPawn : WPawn) ||
        pos.piece(checkSq) == (THEM == Black ? BKnight : WKnight)
        );
      break;
    case (THEM == Black ? BLance : WLance) :
      bannedKingToBB |= lanceAttackToEdge(THEM, checkSq);
      break;
    case (THEM == Black ? BSilver : WSilver) :
      bannedKingToBB |= silverAttack(THEM, checkSq);
      break;
    case (THEM == Black ? BGold : WGold) :
    case (THEM == Black ? BProPawn : WProPawn) :
    case (THEM == Black ? BProLance : WProLance) :
    case (THEM == Black ? BProKnight : WProKnight) :
    case (THEM == Black ? BProSilver : WProSilver) :
      bannedKingToBB |= goldAttack(THEM, checkSq);
      break;
    case (THEM == Black ? BBishop : WBishop) :
      bannedKingToBB |= bishopAttackToEdge(checkSq);
      break;
    case (THEM == Black ? BHorse : WHorse) :
      bannedKingToBB |= horseAttackToEdge(checkSq);
      break;
    case (THEM == Black ? BRook : WRook) :
      bannedKingToBB |= rookAttackToEdge(checkSq);
      break;
    case (THEM == Black ? BDragon : WDragon) :
      if (squareRelation(checkSq, ksq) & DirecDiag) {
        // 斜めから王手したときは、玉の移動先と王手した駒の間に駒があることがあるので、
        // dragonAttackToEdge(checkSq) は使えない。
        bannedKingToBB |= pos.attacksFrom<Dragon>(checkSq);
      }
      else {
        bannedKingToBB |= dragonAttackToEdge(checkSq);
      }
      break;
    default:
      UNREACHABLE;
    }
  }

  // 部分特殊化
  // 王手回避生成
  // 王手をしている駒による王手は避けるが、
  // 玉の移動先に敵の利きがある場合と、pinされている味方の駒を動かした場合、非合法手を生成する。
  // そのため、pseudo legal である。
  template <Color US, bool ALL> struct GenerateMoves<Evasion, US, ALL> {
    /*FORCE_INLINE*/ MoveStack* operator () (MoveStack* moveStackList, const Position& pos) {
      assert(pos.isOK());
      assert(pos.inCheck());

      const Square ksq = pos.kingSquare(US);
      const Color Them = oppositeColor(US);
      const Bitboard checkers = pos.checkersBB();
      Bitboard bb = checkers;
      Bitboard bannedKingToBB = allZeroBB();
      int checkersNum = 0;
      Square checkSq;

      // 玉が逃げられない位置の bitboard を生成する。
      // 絶対に王手が掛かっているので、while ではなく、do while
      do {
        checkSq = bb.firstOneFromI9();
        assert(pieceToColor(pos.piece(checkSq)) == Them);
        ++checkersNum;
        makeBannedKingTo<Them>(bannedKingToBB, pos, checkSq, ksq);
      } while (bb.isNot0());

      // 玉が移動出来る移動先を格納。
      bb = bannedKingToBB.notThisAnd(pos.bbOf(US).notThisAnd(kingAttack(ksq)));
      while (bb.isNot0()) {
        const Square to = bb.firstOneFromI9();
        // 移動先に相手駒の利きがあるか調べずに指し手を生成する。
        // attackersTo() が重いので、movePicker か search で合法手か調べる。
        (*moveStackList++).move = makeNonPromoteMove<Capture>(King, ksq, to, pos);
      }

      // 両王手なら、玉を移動するしか回避方法は無い。
      // 玉の移動は生成したので、ここで終了
      if (1 < checkersNum) {
        return moveStackList;
      }

      // 王手している駒を玉以外で取る手の生成。
      // pin されているかどうかは movePicker か search で調べる。
      const Bitboard target1 = betweenBB(checkSq, ksq);
      const Bitboard target2 = target1 | checkers;
      moveStackList = GeneratePieceMoves<Evasion, Pawn, US, ALL>()(moveStackList, pos, target2, ksq);
      moveStackList = GeneratePieceMoves<Evasion, Lance, US, ALL>()(moveStackList, pos, target2, ksq);
      moveStackList = GeneratePieceMoves<Evasion, Knight, US, ALL>()(moveStackList, pos, target2, ksq);
      moveStackList = GeneratePieceMoves<Evasion, Silver, US, ALL>()(moveStackList, pos, target2, ksq);
      moveStackList = GeneratePieceMoves<Evasion, Bishop, US, ALL>()(moveStackList, pos, target2, ksq);
      moveStackList = GeneratePieceMoves<Evasion, Rook, US, ALL>()(moveStackList, pos, target2, ksq);
      moveStackList = GeneratePieceMoves<Evasion, GoldHorseDragon, US, ALL>()(moveStackList, pos, target2, ksq);

      if (target1.isNot0()) {
        moveStackList = generateDropMoves<US>(moveStackList, pos, target1);
      }

      return moveStackList;
    }
  };

  // 部分特殊化
  // 王手が掛かっていないときの指し手生成
  // これには、玉が相手駒の利きのある地点に移動する自殺手と、pin されている駒を動かす自殺手を含む。
  // ここで生成した手は pseudo legal
  template <Color US> struct GenerateMoves<NonEvasion, US> {
    /*FORCE_INLINE*/ MoveStack* operator () (MoveStack* moveStackList, const Position& pos) {
      Bitboard target = pos.emptyBB();

      moveStackList = generateDropMoves<US>(moveStackList, pos, target);
      target |= pos.bbOf(oppositeColor(US));
      const Square ksq = pos.kingSquare(oppositeColor(US));

      moveStackList = GeneratePieceMoves<NonEvasion, Pawn, US, false>()(moveStackList, pos, target, ksq);
      moveStackList = GeneratePieceMoves<NonEvasion, Lance, US, false>()(moveStackList, pos, target, ksq);
      moveStackList = GeneratePieceMoves<NonEvasion, Knight, US, false>()(moveStackList, pos, target, ksq);
      moveStackList = GeneratePieceMoves<NonEvasion, Silver, US, false>()(moveStackList, pos, target, ksq);
      moveStackList = GeneratePieceMoves<NonEvasion, Bishop, US, false>()(moveStackList, pos, target, ksq);
      moveStackList = GeneratePieceMoves<NonEvasion, Rook, US, false>()(moveStackList, pos, target, ksq);
      moveStackList = GeneratePieceMoves<NonEvasion, GoldHorseDragon, US, false>()(moveStackList, pos, target, ksq);
      moveStackList = GeneratePieceMoves<NonEvasion, King, US, false>()(moveStackList, pos, target, ksq);

      return moveStackList;
    }
  };

  // 部分特殊化
  // 連続王手の千日手以外の反則手を排除した合法手生成
  // そんなに速度が要求されるところでは呼ばない。
  template <Color US> struct GenerateMoves<Legal, US> {
    FORCE_INLINE MoveStack* operator () (MoveStack* moveStackList, const Position& pos) {
      MoveStack* curr = moveStackList;
      const Bitboard pinned = pos.pinnedBB();

      moveStackList = pos.inCheck() ?
        GenerateMoves<Evasion, US>()(moveStackList, pos) : GenerateMoves<NonEvasion, US>()(moveStackList, pos);

      // 玉の移動による自殺手と、pinされている駒の移動による自殺手を削除
      while (curr != moveStackList) {
        if (!pos.pseudoLegalMoveIsLegal<false, false, false>(curr->move, pinned)) {
          curr->move = (--moveStackList)->move;
        }
        else {
          ++curr;
        }
      }

      return moveStackList;
    }
  };

  // 部分特殊化
  // Evasion のときに歩、飛、角と、香の2段目の不成も生成する。
  template <Color US> struct GenerateMoves<LegalAll, US> {
    FORCE_INLINE MoveStack* operator () (MoveStack* moveStackList, const Position& pos) {
      MoveStack* curr = moveStackList;
      const Bitboard pinned = pos.pinnedBB();

      moveStackList = pos.inCheck() ?
        GenerateMoves<Evasion, US, true>()(moveStackList, pos) : GenerateMoves<NonEvasion, US>()(moveStackList, pos);

      // 玉の移動による自殺手と、pinされている駒の移動による自殺手を削除
      while (curr != moveStackList) {
        if (!pos.pseudoLegalMoveIsLegal<false, false, true>(curr->move, pinned)) {
          curr->move = (--moveStackList)->move;
        }
        else {
          ++curr;
        }
      }

      return moveStackList;
    }
  };
}

template <MoveType MT>
MoveStack* generateMoves(MoveStack* moveStackList, const Position& pos) {
  return (pos.turn() == Black ?
    generate_moves::GenerateMoves<MT, Black>()(moveStackList, pos) : generate_moves::GenerateMoves<MT, White>()(moveStackList, pos));
}
template <MoveType MT>
MoveStack* generateMoves(MoveStack* moveStackList, const Position& pos, const Square to) {
  return generate_moves::generateRecaptureMoves(moveStackList, pos, to, pos.turn());
}

// 明示的なインスタンス化
// これが無いと、他のファイルから呼んだ時に、
// 実体が無いためにリンクエラーになる。
// ちなみに、特殊化されたテンプレート関数は、明示的なインスタンス化の必要はない。
// 実装を cpp に置くことで、コンパイル時間の短縮が出来る。
//template MoveStack* generateMoves<Capture           >(MoveStack* moveStackList, const Position& pos);
//template MoveStack* generateMoves<NonCapture        >(MoveStack* moveStackList, const Position& pos);
template MoveStack* generateMoves<Drop              >(MoveStack* moveStackList, const Position& pos);
template MoveStack* generateMoves<CapturePlusPro    >(MoveStack* moveStackList, const Position& pos);
template MoveStack* generateMoves<NonCaptureMinusPro>(MoveStack* moveStackList, const Position& pos);
template MoveStack* generateMoves<Evasion           >(MoveStack* moveStackList, const Position& pos);
template MoveStack* generateMoves<NonEvasion        >(MoveStack* moveStackList, const Position& pos);
template MoveStack* generateMoves<Legal             >(MoveStack* moveStackList, const Position& pos);
template MoveStack* generateMoves<LegalAll          >(MoveStack* moveStackList, const Position& pos);
template MoveStack* generateMoves<Recapture         >(MoveStack* moveStackList, const Position& pos, const Square to);

template MoveStack* generate_moves::generateDropMoves20151211<Black>(MoveStack* moveStackList, const Position& pos, const Bitboard& target);
template MoveStack* generate_moves::generateDropMoves20151211<White>(MoveStack* moveStackList, const Position& pos, const Bitboard& target);
template MoveStack* generate_moves::generateDropMoves<Black>(MoveStack* moveStackList, const Position& pos, const Bitboard& target);
template MoveStack* generate_moves::generateDropMoves<White>(MoveStack* moveStackList, const Position& pos, const Bitboard& target);