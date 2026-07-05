import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("nnue_arch_gen.py")


class NnueArchitectureGeneratorTest(unittest.TestCase):
    def test_generates_progress_layer_stack_sfnn_architecture(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "SFNN_halfka2_768_7_32_p",
                    temp_dir,
                ],
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

            generated = Path(temp_dir, "SFNN_halfka2_768_7_32_p.h").read_text(encoding="utf-8")
            self.assertIn('#include "../features/half_ka_hm2.h"', generated)
            self.assertIn("Features::HalfKA_hm2<Features::Side::kFriend>", generated)
            self.assertIn("constexpr IndexType kTransformedFeatureDimensions = 768;", generated)
            self.assertIn("constexpr int LayerStacks = 8;", generated)
            self.assertIn("constexpr IndexType kHidden1Dims = 7;", generated)
            self.assertIn("constexpr IndexType kHidden2Dims = 32;", generated)
            self.assertIn("#define NNUE_PROGRESS_LAYER_STACKS", generated)
            self.assertIn('return "SFNN_HALFKA2_768_7_32_P";', generated)


if __name__ == "__main__":
    unittest.main()
