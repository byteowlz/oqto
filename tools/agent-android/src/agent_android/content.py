"""Content Provider access - contacts, SMS, calendar, media, custom providers."""

from typing import Optional

from .device import get_device


def query_provider(
    uri: str,
    projection: Optional[str] = None,
    where: Optional[str] = None,
    sort: Optional[str] = None,
) -> str:
    """Query a content provider.

    Args:
        uri: Content URI (e.g., content://contacts/phones)
        projection: Columns to return (colon-separated, e.g., "display_name:number")
        where: WHERE clause
        sort: ORDER BY clause

    Returns:
        Query results as string.

    Examples:
        # Get all contacts
        query_provider("content://contacts/phones")

        # Get SMS inbox
        query_provider("content://sms/inbox")

        # Get calendar events
        query_provider("content://com.android.calendar/events")

        # Custom app provider
        query_provider("content://com.app.provider/data")
    """
    device = get_device()

    cmd = f"content query --uri {uri}"

    if projection:
        cmd += f" --projection {projection}"

    if where:
        # Escape the WHERE clause
        where_escaped = where.replace('"', '\\"')
        cmd += f' --where "{where_escaped}"'

    if sort:
        cmd += f' --sort "{sort}"'

    result = device.shell(cmd)
    return result.output


def insert_provider(uri: str, values: str) -> str:
    """Insert into a content provider.

    Args:
        uri: Content URI
        values: Values as key=value pairs (space-separated)
                Format: --bind key:type:value
                Types: s (string), i (int), l (long), f (float), d (double), b (boolean)

    Returns:
        Insert result.

    Example:
        insert_provider(
            "content://contacts/data",
            "--bind display_name:s:John --bind number:s:555-1234"
        )
    """
    device = get_device()

    cmd = f"content insert --uri {uri} {values}"
    result = device.shell(cmd)
    return result.output


def delete_provider(uri: str, where: Optional[str] = None) -> str:
    """Delete from a content provider.

    Args:
        uri: Content URI
        where: WHERE clause to filter what to delete

    Returns:
        Delete result.

    Example:
        # Delete all call logs
        delete_provider("content://call_log/calls")

        # Delete specific SMS
        delete_provider("content://sms", where="_id=123")
    """
    device = get_device()

    cmd = f"content delete --uri {uri}"

    if where:
        where_escaped = where.replace('"', '\\"')
        cmd += f' --where "{where_escaped}"'

    result = device.shell(cmd)
    return result.output


def update_provider(uri: str, values: str, where: Optional[str] = None) -> str:
    """Update content provider.

    Args:
        uri: Content URI
        values: Values to update (--bind format)
        where: WHERE clause

    Returns:
        Update result.
    """
    device = get_device()

    cmd = f"content update --uri {uri} {values}"

    if where:
        where_escaped = where.replace('"', '\\"')
        cmd += f' --where "{where_escaped}"'

    result = device.shell(cmd)
    return result.output


# Common content provider helpers


def get_contacts() -> str:
    """Get all contacts."""
    return query_provider(
        "content://com.android.contacts/contacts", projection="display_name:lookup"
    )


def get_contact_phones() -> str:
    """Get all contact phone numbers."""
    return query_provider(
        "content://com.android.contacts/data",
        projection="display_name:data1",
        where="mimetype='vnd.android.cursor.item/phone_v2'",
    )


def get_sms_inbox() -> str:
    """Get SMS inbox."""
    return query_provider("content://sms/inbox", projection="address:body:date")


def get_sms_sent() -> str:
    """Get sent SMS."""
    return query_provider("content://sms/sent", projection="address:body:date")


def get_call_log() -> str:
    """Get call log."""
    return query_provider("content://call_log/calls", projection="number:type:date:duration")


def get_calendar_events() -> str:
    """Get calendar events."""
    return query_provider(
        "content://com.android.calendar/events", projection="title:dtstart:dtend:eventLocation"
    )


def get_media_images() -> str:
    """Get media images."""
    return query_provider(
        "content://media/external/images/media", projection="_display_name:_data:date_taken"
    )


def get_downloads() -> str:
    """Get downloads."""
    return query_provider(
        "content://media/external/downloads", projection="_display_name:_data:date_added"
    )
