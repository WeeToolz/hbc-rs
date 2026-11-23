"""Check the independent evidence for a Wiiload self-load in Dolphin."""

import unittest

from test_channel_wad_dolphin import validate_self_load_protocol


class SelfLoadProtocolTests(unittest.TestCase):
    def setUp(self):
        self.parent_reply = {"state": "unknown", "session": 1036}
        self.child_reply = {
            "state": "unknown",
            "session": 4332,
            "message": "No retained result for this request",
        }
        self.upload = {
            "status": "written",
            "result": "host_write_complete",
            "error": None,
            "compressed": True,
            "host_written_bytes": 1351033,
            "input_sha256": "child-dol",
        }

    def test_compressed_matching_dol_starts_new_hbc_session(self):
        checks = validate_self_load_protocol(
            self.parent_reply, self.child_reply, self.upload, "child-dol"
        )
        self.assertTrue(all(checks.values()))

    def test_rejects_same_session_after_upload(self):
        same_session = dict(self.child_reply, session=self.parent_reply["session"])
        with self.assertRaisesRegex(RuntimeError, "hbc_session_changed_after_upload"):
            validate_self_load_protocol(
                self.parent_reply, same_session, self.upload, "child-dol"
            )

    def test_rejects_upload_of_a_different_dol(self):
        with self.assertRaisesRegex(RuntimeError, "uploaded_dol_matches_build"):
            validate_self_load_protocol(
                self.parent_reply, self.child_reply, self.upload, "other-dol"
            )

    def test_rejects_uncompressed_sender_run(self):
        uncompressed = dict(self.upload, compressed=False)
        with self.assertRaisesRegex(RuntimeError, "upload_was_compressed"):
            validate_self_load_protocol(
                self.parent_reply, self.child_reply, uncompressed, "child-dol"
            )


if __name__ == "__main__":
    unittest.main()
