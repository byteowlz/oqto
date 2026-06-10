import json
import sys

from PIL import Image, ImageDraw


# Generates diagnostic images with rectangles indicating region boundaries for
# field annotations in PDFs. Refer to forms.md for details.


def generate_diagnostic_image(page_num, field_json_file, source_image, target_image):
    # Input file must follow the format described in forms.md.
    with open(field_json_file, "r") as file_handle:
        data = json.load(file_handle)

        image = Image.open(source_image)
        canvas = ImageDraw.Draw(image)
        box_count = 0

        for field in data["form_fields"]:
            if field["page_number"] == page_num:
                entry_region = field["entry_bounding_box"]
                label_region = field["label_bounding_box"]
                # Draw red rectangle over entry region and blue rectangle over label.
                canvas.rectangle(entry_region, outline="red", width=2)
                canvas.rectangle(label_region, outline="blue", width=2)
                box_count += 2

        image.save(target_image)
        print(
            f"Generated diagnostic image at {target_image} with {box_count} bounding regions"
        )


if __name__ == "__main__":
    if len(sys.argv) != 5:
        print(
            "Usage: generate_diagnostic_image.py [page number] [fields.json file] [input image path] [output image path]"
        )
        sys.exit(1)
    page_num = int(sys.argv[1])
    field_json_file = sys.argv[2]
    source_image = sys.argv[3]
    target_image = sys.argv[4]
    generate_diagnostic_image(page_num, field_json_file, source_image, target_image)
