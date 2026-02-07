using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;

namespace YaneuraOu.CSharp.Engine.Tests;

[TestClass]
public class PositionTests
{
    [TestMethod]
    public void StartSfen_IsYaneuraOuInitialPosition()
    {
        const string expected = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";
        Assert.AreEqual(expected, Position.StartSfen);
    }
}
