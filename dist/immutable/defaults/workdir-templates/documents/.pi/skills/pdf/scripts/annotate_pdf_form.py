import json
import sys

from pypdf import PdfReader, PdfWriter
from pypdf.annotations import FreeText


# Populates PDF documents by adding text annotations defined in fields.json. Refer to forms.md.


def convert_coordinates(
    bounding_region, img_width, img_height, page_width, page_height
):
    """Convert region coordinates from image space to PDF space"""
    # Image coordinates: origin at top-left, y increases downward
    # PDF coordinates: origin at bottom-left, y increases upward
    scale_x = page_width / img_width
    scale_y = page_height / img_height

    left = bounding_region[0] * scale_x
    right = bounding_region[2] * scale_x

    # Invert Y coordinates for PDF
    top = page_height - (bounding_region[1] * scale_y)
    bottom = page_height - (bounding_region[3] * scale_y)

    return left, bottom, right, top


def annotate_pdf_form(source_pdf, fields_file, destination_pdf):
    """Populate the PDF form using data from fields.json"""

    # fields.json format described in forms.md.
    with open(fields_file, "r") as file_handle:
        form_data = json.load(file_handle)

    # Open the PDF
    document = PdfReader(source_pdf)
    generator = PdfWriter()

    # Copy all pages to generator
    generator.append(document)

    # Get PDF dimensions for each page
    page_dimensions = {}
    for idx, page in enumerate(document.pages):
        mediabox = page.mediabox
        page_dimensions[idx + 1] = [mediabox.width, mediabox.height]

    # Process each form field
    annotations = []
    for field in form_data["form_fields"]:
        page_num = field["page_number"]

        # Get page dimensions and convert coordinates.
        page_info = next(p for p in form_data["pages"] if p["page_number"] == page_num)
        img_width = page_info["image_width"]
        img_height = page_info["image_height"]
        page_width, page_height = page_dimensions[page_num]

        converted_region = convert_coordinates(
            field["entry_bounding_box"], img_width, img_height, page_width, page_height
        )

        # Skip empty fields
        if "entry_text" not in field or "text" not in field["entry_text"]:
            continue
        entry_config = field["entry_text"]
        text_content = entry_config["text"]
        if not text_content:
            continue

        font_family = entry_config.get("font", "Arial")
        font_point_size = str(entry_config.get("font_size", 14)) + "pt"
        text_color = entry_config.get("font_color", "000000")

        # Font size/color appears to not work reliably across viewers:
        # https://github.com/py-pdf/pypdf/issues/2084
        annotation = FreeText(
            text=text_content,
            rect=converted_region,
            font=font_family,
            font_size=font_point_size,
            font_color=text_color,
            border_color=None,
            background_color=None,
        )
        annotations.append(annotation)
        # page_number is 0-based for pypdf
        generator.add_annotation(page_number=page_num - 1, annotation=annotation)

    # Save the populated PDF
    with open(destination_pdf, "wb") as output:
        generator.write(output)

    print(f"Successfully populated PDF form and saved to {destination_pdf}")
    print(f"Added {len(annotations)} text annotations")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("Usage: annotate_pdf_form.py [input pdf] [fields.json] [output pdf]")
        sys.exit(1)
    source_pdf = sys.argv[1]
    fields_file = sys.argv[2]
    destination_pdf = sys.argv[3]

    annotate_pdf_form(source_pdf, fields_file, destination_pdf)
