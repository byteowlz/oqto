import os
import sys

from pdf2image import convert_from_path


# Transforms each page of a PDF document into PNG format.


def transform(pdf_document, destination_folder, maximum_dimension=1000):
    pages = convert_from_path(pdf_document, dpi=200)

    for idx, page_image in enumerate(pages):
        # Scale image if necessary to maintain width/height under maximum_dimension
        img_width, img_height = page_image.size
        if img_width > maximum_dimension or img_height > maximum_dimension:
            scale = min(maximum_dimension / img_width, maximum_dimension / img_height)
            new_width = int(img_width * scale)
            new_height = int(img_height * scale)
            page_image = page_image.resize((new_width, new_height))

        destination_path = os.path.join(destination_folder, f"page_{idx + 1}.png")
        page_image.save(destination_path)
        print(
            f"Stored page {idx + 1} as {destination_path} (dimensions: {page_image.size})"
        )

    print(f"Transformed {len(pages)} pages to PNG format")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: pdf_to_png.py [input pdf] [output directory]")
        sys.exit(1)
    pdf_document = sys.argv[1]
    destination_folder = sys.argv[2]
    transform(pdf_document, destination_folder)
