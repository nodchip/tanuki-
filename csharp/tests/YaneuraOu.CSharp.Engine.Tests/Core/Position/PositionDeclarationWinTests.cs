using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Tests.Core.Positioning;

[TestClass]
/// <summary>
/// Position の宣言勝ち判定に関するテストクラス。
/// </summary>
public class PositionDeclarationWinTests
{
    [TestMethod]
    /// <summary>
    /// ルール未設定では宣言勝ちしないことを検証する。
    /// </summary>
    public void DeclarationWin_NoRule_ReturnsNone()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_53);
        pos.put_piece(Piece.W_KING, Square.SQ_11);

        Assert.AreEqual(Move.none().to_u32(), pos.DeclarationWin().to_u32());
    }

    [TestMethod]
    /// <summary>
    /// 27点法で必要点を満たすと勝ち宣言手を返すことを検証する。
    /// </summary>
    public void DeclarationWin_EnoughPoints_ReturnsWin()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b 2R2B4G4S4N4L18P 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_53);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.set_ekr(EnteringKingRule.EKR_27_POINT);

        Assert.AreEqual(Move.win().to_u32(), pos.DeclarationWin().to_u32());
    }

    [TestMethod]
    /// <summary>
    /// 玉が敵陣外なら必要点を満たしても宣言勝ちしないことを検証する。
    /// </summary>
    public void DeclarationWin_KingOutsideEnemyField_ReturnsNone()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b 2R2B4G4S4N4L18P 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_59);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.set_ekr(EnteringKingRule.EKR_27_POINT);

        Assert.AreEqual(Move.none().to_u32(), pos.DeclarationWin().to_u32());
    }
}
