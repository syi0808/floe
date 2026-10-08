import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import check_boundaries


ROOT = Path(__file__).resolve().parents[3]
POLICY = json.loads((ROOT / "tools/architecture/module-dependencies.json").read_text())
FIXTURES = json.loads(
    (Path(__file__).parent / "fixtures/conversation-boundaries-negative.json").read_text()
)


class ConversationBoundaryNegativeFixtures(unittest.TestCase):
    def test_each_forbidden_dependency_fixture_is_rejected(self):
        forbidden = check_boundaries._forbidden_edges(POLICY)
        for fixture in FIXTURES:
            with self.subTest(fixture=fixture["name"]):
                graph = {node: set(dependencies) for node, dependencies in fixture["graph"].items()}
                errors = check_boundaries.graph_errors(graph, forbidden)
                self.assertTrue(
                    any(error.startswith("forbidden path:") for error in errors),
                    f"expected {fixture['name']} to be rejected, got {errors}",
                )


if __name__ == "__main__":
    unittest.main()
