import sys
from pypdf import PdfReader


# Utility for determining whether a PDF contains interactive form fields.
# Refer to forms.md for usage instructions.


document = PdfReader(sys.argv[1])
if document.get_fields():
    print("This PDF contains interactive form fields")
else:
    print(
        "This PDF lacks interactive form fields; manual identification of data entry locations is required"
    )
