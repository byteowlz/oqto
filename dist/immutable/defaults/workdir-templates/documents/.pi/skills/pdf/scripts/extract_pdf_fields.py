import json
import sys

from pypdf import PdfReader


# Retrieves information for interactive form fields in PDF documents and outputs JSON
# for subsequent field population. Refer to forms.md for usage details.


# Matches the format used by PdfReader get_fields and update_page_form_field_values methods.
def extract_full_annotation_id(annotation):
    parts = []
    while annotation:
        name = annotation.get("/T")
        if name:
            parts.append(name)
        annotation = annotation.get("/Parent")
    return ".".join(reversed(parts)) if parts else None


def build_field_record(field, field_id):
    record = {"field_id": field_id}
    field_type = field.get("/FT")
    if field_type == "/Tx":
        record["type"] = "text"
    elif field_type == "/Btn":
        record["type"] = "checkbox"  # radio groups handled separately
        states = field.get("/_States_", [])
        if len(states) == 2:
            # "/Off" appears to always be the unchecked value, per
            # https://opensource.adobe.com/dc-acrobat-sdk-docs/standards/pdfstandards/pdf/PDF32000_2008.pdf#page=448
            # It can be either first or second in the "/_States_" list.
            if "/Off" in states:
                record["checked_value"] = (
                    states[0] if states[0] != "/Off" else states[1]
                )
                record["unchecked_value"] = "/Off"
            else:
                print(
                    f"Unexpected state values for checkbox `${field_id}`. Its checked and unchecked values may not be correct; if you're trying to check it, visually verify the results."
                )
                record["checked_value"] = states[0]
                record["unchecked_value"] = states[1]
    elif field_type == "/Ch":
        record["type"] = "choice"
        states = field.get("/_States_", [])
        record["choice_options"] = [
            {
                "value": state[0],
                "text": state[1],
            }
            for state in states
        ]
    else:
        record["type"] = f"unknown ({field_type})"
    return record


# Returns a collection of interactive PDF fields:
# [
#   {
#     "field_id": "name",
#     "page": 1,
#     "type": ("text", "checkbox", "radio_group", or "choice")
#     // Per-type additional fields described in forms.md
#   },
# ]
def retrieve_field_data(document: PdfReader):
    fields = document.get_fields()

    field_data_by_id = {}
    potential_radio_names = set()

    for field_id, field in fields.items():
        # Skip container fields with children, except potential radio button parents.
        if field.get("/Kids"):
            if field.get("/FT") == "/Btn":
                potential_radio_names.add(field_id)
            continue
        field_data_by_id[field_id] = build_field_record(field, field_id)

    # Region coordinates are stored in page annotation objects.

    # Radio button options have separate annotations for each choice;
    # all choices share the same field name.
    # See https://westhealth.github.io/exploring-fillable-forms-with-pdfrw.html
    radio_data_by_id = {}

    for page_idx, page in enumerate(document.pages):
        annotations = page.get("/Annots", [])
        for annotation in annotations:
            field_id = extract_full_annotation_id(annotation)
            if field_id in field_data_by_id:
                field_data_by_id[field_id]["page"] = page_idx + 1
                field_data_by_id[field_id]["rect"] = annotation.get("/Rect")
            elif field_id in potential_radio_names:
                try:
                    # annotation['/AP']['/N'] should have two items. One is '/Off',
                    # the other is the active value.
                    active_values = [v for v in annotation["/AP"]["/N"] if v != "/Off"]
                except KeyError:
                    continue
                if len(active_values) == 1:
                    region = annotation.get("/Rect")
                    if field_id not in radio_data_by_id:
                        radio_data_by_id[field_id] = {
                            "field_id": field_id,
                            "type": "radio_group",
                            "page": page_idx + 1,
                            "radio_options": [],
                        }
                    # Note: at least on macOS 15.7, Preview.app doesn't display selected
                    # radio buttons correctly. (It does if you remove the leading slash
                    # from the value, but that causes them not to appear correctly in
                    # Chrome/Firefox/Acrobat/etc).
                    radio_data_by_id[field_id]["radio_options"].append(
                        {
                            "value": active_values[0],
                            "rect": region,
                        }
                    )

    # Some PDFs contain form field definitions without corresponding annotations,
    # making location determination impossible. Ignore these fields for now.
    located_fields = []
    for field_record in field_data_by_id.values():
        if "page" in field_record:
            located_fields.append(field_record)
        else:
            print(
                f"Unable to determine location for field id: {field_record.get('field_id')}, ignoring"
            )

    # Sort by page number, then Y position (inverted in PDF coordinate system), then X.
    def sort_key(field):
        if "radio_options" in field:
            region = field["radio_options"][0]["rect"] or [0, 0, 0, 0]
        else:
            region = field.get("rect") or [0, 0, 0, 0]
        adjusted = [-region[1], region[0]]
        return [field.get("page"), adjusted]

    all_fields = located_fields + list(radio_data_by_id.values())
    all_fields.sort(key=sort_key)

    return all_fields


def export_field_data(pdf_file: str, json_file: str):
    document = PdfReader(pdf_file)
    field_data = retrieve_field_data(document)
    with open(json_file, "w") as file_handle:
        json.dump(field_data, file_handle, indent=2)
    print(f"Exported {len(field_data)} fields to {json_file}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: extract_pdf_fields.py [input pdf] [output json]")
        sys.exit(1)
    export_field_data(sys.argv[1], sys.argv[2])
