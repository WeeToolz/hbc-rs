"""Check the encoded async disc acceptance outcomes."""

import importlib.util
from pathlib import Path
import unittest


SPEC = importlib.util.spec_from_file_location(
    "production_acceptance", Path(__file__).with_name("test_production_acceptance_dolphin.py")
)
PRODUCTION_ACCEPTANCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PRODUCTION_ACCEPTANCE)


class DiscAcceptanceResultTests(unittest.TestCase):
    def test_successful_open_stop_and_close_callbacks(self):
        result = PRODUCTION_ACCEPTANCE.decode_disc_stop_result(0x44070101)
        self.assertEqual(result["disc_callback_mask"], 7)
        self.assertTrue(result["disc_open_succeeded"])
        self.assertTrue(result["disc_stop_succeeded"])

    def test_open_failure_is_distinct_from_successful_motor_stop(self):
        result = PRODUCTION_ACCEPTANCE.decode_disc_stop_result(0x44010000)
        self.assertEqual(result["disc_callback_mask"], 1)
        self.assertFalse(result["disc_open_succeeded"])
        self.assertFalse(result["disc_stop_succeeded"])

    def test_rejects_failed_or_incomplete_callback_chains(self):
        for result in (0, 0x44010101, 0x44070100, 0x44050000):
            with self.subTest(result=result), self.assertRaises(RuntimeError):
                PRODUCTION_ACCEPTANCE.decode_disc_stop_result(result)


if __name__ == "__main__":
    unittest.main()
