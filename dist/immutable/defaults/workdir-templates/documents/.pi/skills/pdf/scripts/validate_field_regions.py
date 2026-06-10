from dataclasses import dataclass
import json
import sys


# Utility to validate that field boundary definitions in JSON files
# do not contain overlapping regions. Refer to forms.md for details.


@dataclass
class RegionAndField:
    region: list[float]
    region_type: str
    field: dict


# Returns a collection of diagnostic messages printed to stdout.
def validate_region_overlaps(field_json_stream) -> list[str]:
    diagnostics = []
    data = json.load(field_json_stream)
    diagnostics.append(f"Loaded {len(data['form_fields'])} fields")

    def regions_overlap(a, b):
        separated_horizontally = a[0] >= b[2] or a[2] <= b[0]
        separated_vertically = a[1] >= b[3] or a[3] <= b[1]
        return not (separated_horizontally or separated_vertically)

    regions_and_fields = []
    for entry in data["form_fields"]:
        regions_and_fields.append(
            RegionAndField(entry["label_bounding_box"], "label", entry)
        )
        regions_and_fields.append(
            RegionAndField(entry["entry_bounding_box"], "entry", entry)
        )

    found_issues = False
    for i, first in enumerate(regions_and_fields):
        # This is O(N^2); optimization possible if performance becomes critical.
        for j in range(i + 1, len(regions_and_fields)):
            second = regions_and_fields[j]
            if first.field["page_number"] == second.field[
                "page_number"
            ] and regions_overlap(first.region, second.region):
                found_issues = True
                if first.field is second.field:
                    diagnostics.append(
                        f"FAILURE: overlap between label and entry regions for `{first.field['description']}` ({first.region}, {second.region})"
                    )
                else:
                    diagnostics.append(
                        f"FAILURE: overlap between {first.region_type} region for `{first.field['description']}` ({first.region}) and {second.region_type} region for `{second.field['description']}` ({second.region})"
                    )
                if len(diagnostics) >= 20:
                    diagnostics.append(
                        "Stopping validation; correct region definitions and retry"
                    )
                    return diagnostics
        if first.region_type == "entry":
            if "entry_text" in first.field:
                text_size = first.field["entry_text"].get("font_size", 14)
                region_height = first.region[3] - first.region[1]
                if region_height < text_size:
                    found_issues = True
                    diagnostics.append(
                        f"FAILURE: entry region height ({region_height}) for `{first.field['description']}` is insufficient for text content (font size: {text_size}). Increase region height or reduce font size."
                    )
                    if len(diagnostics) >= 20:
                        diagnostics.append(
                            "Stopping validation; correct region definitions and retry"
                        )
                        return diagnostics

    if not found_issues:
        diagnostics.append("SUCCESS: All region definitions are valid")
    return diagnostics


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: validate_field_regions.py [fields.json]")
        sys.exit(1)
    # Input file must follow the format described in forms.md.
    with open(sys.argv[1]) as file_handle:
        diagnostics = validate_region_overlaps(file_handle)
    for message in diagnostics:
        print(message)
