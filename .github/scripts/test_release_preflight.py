import unittest

from release_preflight import REQUIRED_WORKFLOWS, check_runs, check_versions


class ReleasePreflightTests(unittest.TestCase):
    def test_versions_match_every_workspace_member(self):
        metadata = {"workspace_members":["a", "b"], "packages":[
            {"id":"a", "name":"one", "version":"1.2.3"}, {"id":"b", "name":"two", "version":"1.2.3"}]}
        check_versions(metadata, "v1.2.3")
        for tag in ["v1.2", "v1.2.4", "not-a-release"]:
            with self.assertRaises(ValueError): check_versions(metadata, tag)
        metadata["packages"][1]["version"] = "1.2.4"
        with self.assertRaises(ValueError): check_versions(metadata, "v1.2.3")

    def test_only_successful_latest_runs_at_exact_commit_pass(self):
        runs = [{"path":path, "id":i, "head_sha":"commit", "created_at":"1", "status":"completed", "conclusion":"success"}
                for i,path in enumerate(REQUIRED_WORKFLOWS)]
        check_runs(runs, "commit")
        with self.assertRaises(ValueError): check_runs(runs, "other-commit")
        with self.assertRaises(ValueError): check_runs(runs[:-1], "commit")
        for state, conclusion in [("in_progress",None), ("completed","failure"), ("completed","cancelled")]:
            later = {**runs[0], "id":100, "created_at":"2", "status":state, "conclusion":conclusion}
            with self.assertRaises(ValueError): check_runs(runs+[later], "commit")


if __name__ == "__main__":
    unittest.main()
