import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import visual_review_bundle as bundle


class VisualReviewBundleTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='aether-review-bundle-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.reports = self.root / 'reports'
        self.matrix = self.root / 'matrix.json'
        self.output = self.root / 'review'
        self.case = {'id': 't4_water_on', 'criteria': ['water visible'], 'scene': 'scenes/water.ron'}
        self.matrix.write_text(json.dumps([self.case]))
        self.case_dir = self.reports / 'run-1' / self.case['id']
        self.case_dir.mkdir(parents=True)
        self.image = self.case_dir / 'output.png'
        Image.new('RGBA', (8, 8), (10, 20, 30, 255)).save(self.image)
        self.sha = hashlib.sha256(self.image.read_bytes()).hexdigest()
        candidate = {'id': self.case['id'], 'status': 'reference-pending', 'criteria_status': 'needs-review',
                     'case': self.case, 'candidate_reference': {'sha256': self.sha, 'source_commit': 'abc123'},
                     'launcher': {'exit_code': 0, 'group_empty': True}}
        (self.case_dir / 'candidate.json').write_text(json.dumps(candidate))
        (self.case_dir / 'report.html').write_text('<h1>candidate report</h1>')

    def test_build_groups_candidate_and_emits_contact_sheet_and_html(self):
        summary = bundle.build(self.matrix, self.reports, self.output)
        self.assertEqual(summary['candidate_count'], 1)
        self.assertIn('T4', summary['groups'])
        self.assertTrue((self.output / 'index.html').is_file())
        self.assertTrue((self.output / 'contact-T4.png').is_file())
        self.assertIn(self.sha, (self.output / 'index.html').read_text())

    def test_review_ledger_is_bound_to_current_candidate_hash(self):
        bundle.build(self.matrix, self.reports, self.output)
        bundle.record_review(self.output, self.case['id'], self.sha, 'pass', 'matches acceptance criteria')
        ledger = json.loads((self.output / 'review-ledger.json').read_text())
        self.assertEqual(ledger[self.case['id']]['status'], 'pass')
        bundle.build(self.matrix, self.reports, self.output)
        self.assertIn('审核记录：pass', (self.output / 'index.html').read_text())
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            bundle.record_review(self.output, self.case['id'], 'stale', 'pass', 'stale candidate')

    def test_only_explicit_pass_is_eligible_for_promotion(self):
        bundle.build(self.matrix, self.reports, self.output)
        with self.assertRaisesRegex(ValueError, 'explicit human pass'):
            bundle.approved_candidates(self.output)
        bundle.record_review(self.output, self.case['id'], self.sha, 'insufficient', 'need another view')
        with self.assertRaisesRegex(ValueError, 'explicit human pass'):
            bundle.approved_candidates(self.output)
        bundle.record_review(self.output, self.case['id'], self.sha, 'pass', 'reviewed')
        self.assertEqual(bundle.approved_candidates(self.output), [self.case['id']])

    def test_regular_run_is_reviewable_but_not_promotable(self):
        (self.case_dir / 'candidate.json').unlink()
        aggregate = {'id': self.case['id'], 'status': 'reference-pending', 'case': self.case,
                     'launcher': {'exit_code': 0, 'group_empty': True, 'diagnostic': None}}
        (self.case_dir / 'aggregate.json').write_text(json.dumps(aggregate))
        bundle.build(self.matrix, self.reports, self.output)
        bundle.record_review(self.output, self.case['id'], self.sha, 'pass', 'visual review')
        self.assertEqual(bundle._bundle_candidates(self.output)[self.case['id']]['promotable'], False)
        with self.assertRaisesRegex(ValueError, 'explicit human pass'):
            bundle.approved_candidates(self.output)

    def test_candidate_provenance_file_wins_over_same_run_aggregate(self):
        aggregate = {'id': self.case['id'], 'status': 'reference-pending', 'case': self.case,
                     'launcher': {'exit_code': 0, 'group_empty': True, 'diagnostic': None}}
        (self.case_dir / 'aggregate.json').write_text(json.dumps(aggregate))
        bundle.build(self.matrix, self.reports, self.output)
        self.assertTrue(bundle._bundle_candidates(self.output)[self.case['id']]['promotable'])


if __name__ == '__main__':
    unittest.main(verbosity=2)
