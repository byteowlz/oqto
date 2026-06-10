import json
import sys

from pypdf import PdfReader, PdfWriter

from extract_pdf_fields import retrieve_field_data


# Populates interactive form fields in PDF documents. Refer to forms.md for usage.


def populate_pdf_fields(source_pdf: str, values_json: str, destination_pdf: str):
    with open(values_json) as file_handle:
        field_values = json.load(file_handle)
    # Group by page number.
    values_by_page = {}
    for entry in field_values:
        if "value" in entry:
            field_id = entry["field_id"]
            page_num = entry["page"]
            if page_num not in values_by_page:
                values_by_page[page_num] = {}
            values_by_page[page_num][field_id] = entry["value"]

    document = PdfReader(source_pdf)

    has_issues = False
    field_data = retrieve_field_data(document)
    fields_by_id = {f["field_id"]: f for f in field_data}
    for entry in field_values:
        existing = fields_by_id.get(entry["field_id"])
        if not existing:
            has_issues = True
            print(f"ERROR: `{entry['field_id']}` is not a valid field ID")
        elif entry["page"] != existing["page"]:
            has_issues = True
            print(
                f"ERROR: Incorrect page number for `{entry['field_id']}` (got {entry['page']}, expected {existing['page']})"
            )
        else:
            if "value" in entry:
                error = validate_field_value(existing, entry["value"])
                if error:
                    print(error)
                    has_issues = True
    if has_issues:
        sys.exit(1)

    generator = PdfWriter(clone_from=document)
    for page_num, values in values_by_page.items():
        generator.update_page_form_field_values(
            generator.pages[page_num - 1], values, auto_regenerate=False
        )

    # This appears necessary for many PDF viewers to display form values correctly.
    # It may cause the viewer to show a "save changes" dialog even if the user doesn't make any changes.
    generator.set_need_appearances_writer(True)

    with open(destination_pdf, "wb") as file_handle:
        generator.write(file_handle)


def validate_field_value(field_data, value):
    field_type = field_data["type"]
    field_id = field_data["field_id"]
    if field_type == "checkbox":
        checked = field_data["checked_value"]
        unchecked = field_data["unchecked_value"]
        if value != checked and value != unchecked:
            return f'ERROR: Invalid value "{value}" for checkbox field "{field_id}". The checked value is "{checked}" and the unchecked value is "{unchecked}"'
    elif field_type == "radio_group":
        valid_options = [opt["value"] for opt in field_data["radio_options"]]
        if value not in valid_options:
            return f'ERROR: Invalid value "{value}" for radio group field "{field_id}". Valid values are: {valid_options}'
    elif field_type == "choice":
        valid_choices = [opt["value"] for opt in field_data["choice_options"]]
        if value not in valid_choices:
            return f'ERROR: Invalid value "{value}" for choice field "{field_id}". Valid values are: {valid_choices}'
    return None


# pypdf (at least version 5.7.0) contains a bug when setting values for selection list fields.
# In _writer.py around line 966:
#
# if field.get(FA.FT, "/Tx") == "/Ch" and field_flags & FA.FfBits.Combo == 0:
#     txt = "\n".join(annotation.get_inherited(FA.Opt, []))
#
# The problem is that for selection lists, `get_inherited` returns a list of two-element lists like
# [["value1", "Text 1"], ["value2", "Text 2"], ...]
# This causes `join` to throw a TypeError because it expects an iterable of strings.
# The workaround is to patch `get_inherited` to return a list of the value strings.
# We call the original method and adjust the return value only if the argument to `get_inherited`
# is `FA.Opt` and if the return value is a list of two-element lists.
def apply_pypdf_patch():
    from pypdf.generic import DictionaryObject
    from pypdf.constants import FieldDictionaryAttributes

    original_method = DictionaryObject.get_inherited

    def patched_method(self, key: str, default=None):
        result = original_method(self, key, default)
        if key == FieldDictionaryAttributes.Opt:
            if isinstance(result, list) and all(
                isinstance(v, list) and len(v) == 2 for v in result
            ):
                result = [r[0] for r in result]
        return result

    DictionaryObject.get_inherited = patched_method


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print(
            "Usage: populate_pdf_fields.py [input pdf] [field_values.json] [output pdf]"
        )
        sys.exit(1)
    apply_pypdf_patch()
    source_pdf = sys.argv[1]
    values_json = sys.argv[2]
    destination_pdf = sys.argv[3]
    populate_pdf_fields(source_pdf, values_json, destination_pdf)
