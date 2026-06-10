import unittest
import json
import io
from validate_field_regions import validate_region_overlaps


# Currently not executed automatically in CI; intended for documentation and manual verification.
class TestRegionValidation(unittest.TestCase):
    def make_json_stream(self, data):
        """Helper to create a JSON stream from data"""
        return io.StringIO(json.dumps(data))

    def test_no_overlaps(self):
        """Test case with no region overlaps"""
        data = {
            "form_fields": [
                {
                    "description": "Name",
                    "page_number": 1,
                    "label_bounding_box": [10, 10, 50, 30],
                    "entry_bounding_box": [60, 10, 150, 30],
                },
                {
                    "description": "Email",
                    "page_number": 1,
                    "label_bounding_box": [10, 40, 50, 60],
                    "entry_bounding_box": [60, 40, 150, 60],
                },
            ]
        }

        stream = self.make_json_stream(data)
        diagnostics = validate_region_overlaps(stream)
        self.assertTrue(any("SUCCESS" in msg for msg in diagnostics))
        self.assertFalse(any("FAILURE" in msg for msg in diagnostics))

    def test_label_entry_overlap_same_field(self):
        """Test overlap between label and entry of the same field"""
        data = {
            "form_fields": [
                {
                    "description": "Name",
                    "page_number": 1,
                    "label_bounding_box": [10, 10, 60, 30],
                    "entry_bounding_box": [50, 10, 150, 30],  # Overlaps with label
                }
            ]
        }

        stream = self.make_json_stream(data)
        diagnostics = validate_region_overlaps(stream)
        self.assertTrue(
            any("FAILURE" in msg and "overlap" in msg for msg in diagnostics)
        )
        self.assertFalse(any("SUCCESS" in msg for msg in diagnostics))

    def test_overlap_between_different_fields(self):
        """Test overlap between regions of different fields"""
        data = {
            "form_fields": [
                {
                    "description": "Name",
                    "page_number": 1,
                    "label_bounding_box": [10, 10, 50, 30],
                    "entry_bounding_box": [60, 10, 150, 30],
                },
                {
                    "description": "Email",
                    "page_number": 1,
                    "label_bounding_box": [
                        40,
                        20,
                        80,
                        40,
                    ],  # Overlaps with Name's boxes
                    "entry_bounding_box": [160, 10, 250, 30],
                },
            ]
        }

        stream = self.make_json_stream(data)
        diagnostics = validate_region_overlaps(stream)
        self.assertTrue(
            any("FAILURE" in msg and "overlap" in msg for msg in diagnostics)
        )
        self.assertFalse(any("SUCCESS" in msg for msg in diagnostics))

    def test_different_pages_no_overlap(self):
        """Test that regions on different pages don't count as overlapping"""
        data = {
            "form_fields": [
                {
                    "description": "Name",
                    "page_number": 1,
                    "label_bounding_box": [10, 10, 50, 30],
                    "entry_bounding_box": [60, 10, 150, 30],
                },
                {
                    "description": "Email",
                    "page_number": 2,
                    "label_bounding_box": [
                        10,
                        10,
                        50,
                        30,
                    ],  # Same coordinates but different page
                    "entry_bounding_box": [60, 10, 150, 30],
                },
            ]
        }

        stream = self.make_json_stream(data)
        diagnostics = validate_region_overlaps(stream)
        self.assertTrue(any("SUCCESS" in msg for msg in diagnostics))
        self.assertFalse(any("FAILURE" in msg for msg in diagnostics))

    def test_entry_height_insufficient(self):
        """Test that entry region height is validated against font size"""
        data = {
            "form_fields": [
                {
                    "description": "Name",
                    "page_number": 1,
                    "label_bounding_box": [10, 10, 50, 30],
                    "entry_bounding_box": [60, 10, 150, 20],  # Height is 10
                    "entry_text": {
                        "font_size": 14  # Font size larger than height
                    },
                }
            ]
        }

        stream = self.make_json_stream(data)
        diagnostics = validate_region_overlaps(stream)
        self.assertTrue(
            any("FAILURE" in msg and "height" in msg for msg in diagnostics)
        )
        self.assertFalse(any("SUCCESS" in msg for msg in diagnostics))

    def test_entry_height_sufficient(self):
        """Test that adequate entry region height passes"""
        data = {
            "form_fields": [
                {
                    "description": "Name",
                    "page_number": 1,
                    "label_bounding_box": [10, 10, 50, 30],
                    "entry_bounding_box": [60, 10, 150, 30],  # Height is 20
                    "entry_text": {
                        "font_size": 14  # Font size smaller than height
                    },
                }
            ]
        }

        stream = self.make_json_stream(data)
        diagnostics = validate_region_overlaps(stream)
        self.assertTrue(any("SUCCESS" in msg for msg in diagnostics))
        self.assertFalse(any("FAILURE" in msg for msg in diagnostics))

    def test_default_font_size(self):
        """Test that default font size is used when not specified"""
        data = {
            "form_fields": [
                {
                    "description": "Name",
                    "page_number": 1,
                    "label_bounding_box": [10, 10, 50, 30],
                    "entry_bounding_box": [60, 10, 150, 20],  # Height is 10
                    "entry_text": {},  # No font_size specified, should use default 14
                }
            ]
        }

        stream = self.make_json_stream(data)
        diagnostics = validate_region_overlaps(stream)
        self.assertTrue(
            any("FAILURE" in msg and "height" in msg for msg in diagnostics)
        )
        self.assertFalse(any("SUCCESS" in msg for msg in diagnostics))

    def test_missing_entry_text(self):
        """Test that missing entry_text doesn't trigger height check"""
        data = {
            "form_fields": [
                {
                    "description": "Name",
                    "page_number": 1,
                    "label_bounding_box": [10, 10, 50, 30],
                    "entry_bounding_box": [
                        60,
                        10,
                        150,
                        20,
                    ],  # Small height but no entry_text
                }
            ]
        }

        stream = self.make_json_stream(data)
        diagnostics = validate_region_overlaps(stream)
        self.assertTrue(any("SUCCESS" in msg for msg in diagnostics))
        self.assertFalse(any("FAILURE" in msg for msg in diagnostics))

    def test_multiple_errors_limit(self):
        """Test that diagnostic messages are limited to prevent excessive output"""
        fields = []
        # Create many overlapping fields
        for i in range(25):
            fields.append(
                {
                    "description": f"Field{i}",
                    "page_number": 1,
                    "label_bounding_box": [10, 10, 50, 30],  # All overlap
                    "entry_bounding_box": [20, 15, 60, 35],  # All overlap
                }
            )

        data = {"form_fields": fields}

        stream = self.make_json_stream(data)
        diagnostics = validate_region_overlaps(stream)
        # Should stop after ~20 messages
        self.assertTrue(any("Stopping" in msg for msg in diagnostics))
        # Should have some FAILURE messages but not hundreds
        failure_count = sum(1 for msg in diagnostics if "FAILURE" in msg)
        self.assertGreater(failure_count, 0)
        self.assertLess(len(diagnostics), 30)  # Should be limited

    def test_edge_touching_regions(self):
        """Test that regions touching at edges don't count as overlapping"""
        data = {
            "form_fields": [
                {
                    "description": "Name",
                    "page_number": 1,
                    "label_bounding_box": [10, 10, 50, 30],
                    "entry_bounding_box": [50, 10, 150, 30],  # Touches at x=50
                }
            ]
        }

        stream = self.make_json_stream(data)
        diagnostics = validate_region_overlaps(stream)
        self.assertTrue(any("SUCCESS" in msg for msg in diagnostics))
        self.assertFalse(any("FAILURE" in msg for msg in diagnostics))


if __name__ == "__main__":
    unittest.main()
