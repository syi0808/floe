package app.floe.floe_client

import android.Manifest
import android.app.Activity
import android.content.ContentUris
import android.content.pm.PackageManager
import android.database.Cursor
import android.os.Build
import android.os.Bundle
import android.provider.CalendarContract
import android.provider.ContactsContract
import io.flutter.plugin.common.BinaryMessenger
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.nio.charset.StandardCharsets
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.Executors
import org.json.JSONObject

internal class AndroidContextChannel(
    private val activity: Activity,
    messenger: BinaryMessenger,
) : MethodChannel.MethodCallHandler {
    private val channel = MethodChannel(messenger, "floe/android_context")
    private val worker = Executors.newSingleThreadExecutor()
    private var permissionResult: MethodChannel.Result? = null
    private var permissionRequestCode = 7000
    @Volatile private var calendarLastView: Map<String, Any?>? = null
    @Volatile private var contactsLastView: Map<String, Any?>? = null
    @Volatile private var calendarLastSuccess: Long? = null
    @Volatile private var contactsLastSuccess: Long? = null

    init {
        channel.setMethodCallHandler(this)
    }

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "connections" -> result.success(connectionSnapshots())
            "requestPermission" -> requestPermission(call, result)
            "readCalendar" -> runWorker(result) { readCalendar(arguments(call)) }
            "readContacts" -> runWorker(result) { readContacts(arguments(call)) }
            else -> result.notImplemented()
        }
    }

    fun onRequestPermissionsResult(requestCode: Int, grantResults: IntArray): Boolean {
        if (requestCode != permissionRequestCode || permissionResult == null) return false
        val pending = permissionResult
        permissionResult = null
        pending?.success(mapOf("granted" to (grantResults.isNotEmpty() && grantResults.all { it == PackageManager.PERMISSION_GRANTED })))
        return true
    }

    fun close() {
        channel.setMethodCallHandler(null)
        permissionResult?.error("cancelled", "Permission request was cancelled.", null)
        permissionResult = null
        worker.shutdownNow()
    }

    private fun requestPermission(call: MethodCall, result: MethodChannel.Result) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) {
            result.error("unsupported", "Android runtime permissions are unavailable.", null)
            return
        }
        if (permissionResult != null) {
            result.error("busy", "Another permission request is active.", null)
            return
        }
        val permissions = when (call.argument<String>("source")) {
            "calendar" -> arrayOf(Manifest.permission.READ_CALENDAR)
            "contacts" -> arrayOf(Manifest.permission.READ_CONTACTS)
            else -> {
                result.error("invalid_input", "Unknown Android context source.", null)
                return
            }
        }
        if (permissions.all { granted(it) }) {
            result.success(mapOf("granted" to true))
            return
        }
        permissionResult = result
        permissionRequestCode += 1
        activity.requestPermissions(permissions, permissionRequestCode)
    }

    private fun readCalendar(arguments: Map<String, Any?>): Map<String, Any?> {
        requirePermission(Manifest.permission.READ_CALENDAR)
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) throw ContextFailure("unsupported", "Bounded provider queries require Android 8 or later.")
        val start = arguments.long("range_start_unix_ms")
        val end = arguments.long("range_end_unix_ms")
        val limit = arguments.int("limit")
        val offset = arguments.string("cursor").ifEmpty { "0" }.toIntOrNull()
            ?: throw ContextFailure("invalid_input", "Calendar cursor is invalid.")
        val calendarIds = arguments.stringList("calendar_ids")
        if (start < 0 || end <= start || end - start > MAX_RANGE_MS || limit !in 1..MAX_CALENDAR_ITEMS || offset !in 0..10_000 || calendarIds.isEmpty() || calendarIds.size > 4 || calendarIds.toSet().size != calendarIds.size || calendarIds.any { !validOpaque(it, 512) }) {
            throw ContextFailure("invalid_input", "Calendar request is outside the bounded scope.")
        }
        val uriBuilder = CalendarContract.Instances.CONTENT_URI.buildUpon()
        ContentUris.appendId(uriBuilder, start)
        ContentUris.appendId(uriBuilder, end)
        val query = Bundle().apply {
            putString(ContentResolverKeys.SELECTION, calendarIds.joinToString(",", prefix = "${CalendarContract.Instances.CALENDAR_ID} IN (", postfix = ")") { "?" })
            putStringArray(ContentResolverKeys.SELECTION_ARGS, calendarIds.toTypedArray())
            putStringArray(ContentResolverKeys.SORT_COLUMNS, arrayOf(CalendarContract.Instances.BEGIN))
            putInt(ContentResolverKeys.SORT_DIRECTION, android.content.ContentResolver.QUERY_SORT_DIRECTION_ASCENDING)
            putInt(ContentResolverKeys.LIMIT, limit + 1)
            putInt(ContentResolverKeys.OFFSET, offset)
        }
        val projection = arrayOf(CalendarContract.Instances.EVENT_ID, CalendarContract.Instances.TITLE, CalendarContract.Instances.BEGIN, CalendarContract.Instances.END, CalendarContract.Instances.ALL_DAY)
        val rows = mutableListOf<Map<String, Any?>>()
        activity.contentResolver.query(uriBuilder.build(), projection, query, null)?.use { cursor ->
            while (cursor.moveToNext() && rows.size <= limit) rows += calendarItem(cursor)
        } ?: throw ContextFailure("unavailable", "Calendar provider returned no cursor.")
        val hasMore = rows.size > limit
        if (hasMore) rows.removeAt(rows.lastIndex)
        val observed = System.currentTimeMillis()
        val view = linkedMapOf<String, Any?>(
            "schema_version" to 1,
            "view_id" to "calendar.timeline",
            "source_handle" to opaqueHandle("calendar.timeline", calendarIds.sorted().joinToString("\u0000")),
            "observed_at_unix_ms" to observed,
            "expires_at_unix_ms" to observed + FRESHNESS_MS,
            "range_start_unix_ms" to start,
            "range_end_unix_ms" to end,
            "coverage_complete" to !hasMore,
        )
        if (hasMore) view["next_cursor"] = (offset + rows.size).toString()
        view["items"] = rows
        requireEncodedLimit(view, MAX_VIEW_BYTES)
        calendarLastView = view
        calendarLastSuccess = observed
        return view
    }

    private fun calendarItem(cursor: Cursor): Map<String, Any?> {
        val identifier = cursor.requiredString(0, 512)
        val title = cursor.getString(1)?.take(MAX_TITLE_CHARS) ?: ""
        val starts = cursor.getLong(2)
        val ends = cursor.getLong(3)
        if (starts < 0 || ends <= starts) throw ContextFailure("invalid_response", "Calendar provider returned an invalid event.")
        return linkedMapOf(
            "evidence_handle" to opaqueHandle("calendar.event", identifier),
            "untrusted_title" to title,
            "starts_at_unix_ms" to starts,
            "ends_at_unix_ms" to ends,
            "all_day" to (cursor.getInt(4) == 1),
        )
    }

    private fun readContacts(arguments: Map<String, Any?>): Map<String, Any?> {
        requirePermission(Manifest.permission.READ_CONTACTS)
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) throw ContextFailure("unsupported", "Bounded provider queries require Android 8 or later.")
        val limit = arguments.int("limit")
        if (limit !in 1..MAX_CONTACT_ITEMS) throw ContextFailure("invalid_input", "Contact limit is invalid.")
        val query = Bundle().apply {
            putStringArray(ContentResolverKeys.SORT_COLUMNS, arrayOf(ContactsContract.Contacts.DISPLAY_NAME_PRIMARY))
            putInt(ContentResolverKeys.SORT_DIRECTION, android.content.ContentResolver.QUERY_SORT_DIRECTION_ASCENDING)
            putInt(ContentResolverKeys.LIMIT, limit + 1)
        }
        val projection = arrayOf(ContactsContract.Contacts._ID, ContactsContract.Contacts.DISPLAY_NAME_PRIMARY)
        val identities = mutableListOf<Map<String, Any?>>()
        var hasMore = false
        activity.contentResolver.query(ContactsContract.Contacts.CONTENT_URI, projection, query, null)?.use { cursor ->
            while (cursor.moveToNext()) {
                val identifier = cursor.requiredString(0, 512)
                val name = cursor.getString(1)?.trim().orEmpty().take(MAX_CONTACT_NAME_CHARS)
                if (name.isEmpty()) continue
                if (identities.size == limit) {
                    hasMore = true
                    break
                }
                val evidence = opaqueHandle("contact.evidence", identifier)
                identities += linkedMapOf(
                    "identity_handle" to opaqueHandle("person.identity", identifier),
                    "display_name" to name,
                    "aliases" to emptyList<String>(),
                    "confidence_millis" to 1000,
                    "evidence_handles" to listOf(evidence),
                )
            }
        } ?: throw ContextFailure("unavailable", "Contacts provider returned no cursor.")
        val observed = System.currentTimeMillis()
        val view = linkedMapOf<String, Any?>(
            "schema_version" to 1,
            "view_id" to "people.identity",
            "source_handle" to opaqueHandle("people", deviceId()),
            "observed_at_unix_ms" to observed,
            "expires_at_unix_ms" to observed + FRESHNESS_MS,
            "coverage_complete" to !hasMore,
            "identities" to identities,
        )
        requireEncodedLimit(view, MAX_PEOPLE_BYTES)
        contactsLastView = view
        contactsLastSuccess = observed
        return view
    }

    private fun connectionSnapshots(): List<Map<String, Any?>> {
        val observed = System.currentTimeMillis()
        calendarLastView = if (granted(Manifest.permission.READ_CALENDAR)) freshView(calendarLastView, observed) else null
        contactsLastView = if (granted(Manifest.permission.READ_CONTACTS)) freshView(contactsLastView, observed) else null
        return listOf(
            connectionSnapshot("calendar.android", "android_calendar", "calendar.events.read", Manifest.permission.READ_CALENDAR, "calendar.timeline", 128, 65_536, calendarLastView, calendarLastSuccess, "items", observed),
            connectionSnapshot("contacts.android", "android_contacts", "contacts.identity.read", Manifest.permission.READ_CONTACTS, "people.identity", 64, 32_768, contactsLastView, contactsLastSuccess, "identities", observed),
        )
    }

    private fun connectionSnapshot(connector: String, provider: String, capability: String, permission: String, viewId: String, maxItems: Int, maxBytes: Int, lastView: Map<String, Any?>?, lastSuccess: Long?, itemsKey: String, observed: Long): Map<String, Any?> {
        val allowed = granted(permission)
        val lastObserved = lastView?.get("observed_at_unix_ms") as? Long
        val lastExpires = lastView?.get("expires_at_unix_ms") as? Long
        val fresh = allowed && lastObserved != null && lastExpires != null && lastExpires > observed
        val connection = linkedMapOf<String, Any?>(
            "schema_version" to 1,
            "connector_id" to connector,
            "state" to when {
                !allowed -> "revoked"
                lastView == null && lastSuccess == null -> "pending"
                fresh -> "ready"
                else -> "unavailable"
            },
            "granted_scopes" to if (allowed) listOf(permission) else emptyList<String>(),
            "observed_at_unix_ms" to observed,
        )
        if (lastSuccess != null) connection["last_success_at_unix_ms"] = lastSuccess
        if (!allowed) connection["last_failure"] = mapOf("kind" to "permission_denied", "observed_at_unix_ms" to observed)
        else if (lastSuccess != null && !fresh) connection["last_failure"] = mapOf("kind" to "stale", "observed_at_unix_ms" to observed)
        val views = if (fresh) {
            val items = lastView?.get(itemsKey) as? List<*> ?: emptyList<Any>()
            listOf(mapOf(
                "schema_version" to 1,
                "view_id" to viewId,
                "source_handle" to lastView?.get("source_handle"),
                "observed_at_unix_ms" to lastObserved,
                "expires_at_unix_ms" to lastExpires,
                "item_count" to items.size,
                "byte_count" to JSONObject(lastView).toString().toByteArray(StandardCharsets.UTF_8).size,
                "provenance_count" to items.size,
            ))
        } else emptyList()
        return mapOf(
            "descriptor" to mapOf(
                "schema_version" to 1,
                "id" to connector,
                "version" to "1.0.0",
                "provider" to provider,
                "execution" to mapOf("kind" to "device", "device_id" to deviceId()),
                "capabilities" to listOf(mapOf("schema_version" to 1, "id" to capability, "version" to "1.0.0", "authority" to "observe", "required_scopes" to listOf(permission), "output_view_id" to viewId)),
                "views" to listOf(mapOf("schema_version" to 1, "id" to viewId, "version" to "1.0.0", "data_class" to "personal", "retention" to "ephemeral", "freshness_ttl_ms" to FRESHNESS_MS, "max_items" to maxItems, "max_bytes" to maxBytes, "provenance_required" to true)),
            ),
            "connection" to connection,
            "views" to views,
        )
    }

    private fun arguments(call: MethodCall): Map<String, Any?> = (call.arguments as? Map<*, *>)?.entries?.associate { it.key.toString() to it.value }
        ?: throw ContextFailure("invalid_input", "Android context arguments are required.")

    private fun runWorker(result: MethodChannel.Result, operation: () -> Any?) {
        worker.execute {
            try {
                val value = operation()
                activity.runOnUiThread { result.success(value) }
            } catch (failure: ContextFailure) {
                activity.runOnUiThread { result.error(failure.code, failure.message, null) }
            } catch (_: SecurityException) {
                activity.runOnUiThread { result.error("permission_denied", "Android source permission was denied.", null) }
            } catch (_: Throwable) {
                activity.runOnUiThread { result.error("unavailable", "Android context source is unavailable.", null) }
            }
        }
    }

    private fun requirePermission(permission: String) {
        if (!granted(permission)) {
            if (permission == Manifest.permission.READ_CALENDAR) calendarLastView = null
            if (permission == Manifest.permission.READ_CONTACTS) contactsLastView = null
            throw ContextFailure("permission_denied", "Android source permission was denied.")
        }
    }

    private fun freshView(view: Map<String, Any?>?, observed: Long): Map<String, Any?>? = view?.takeIf { (it["expires_at_unix_ms"] as? Long)?.let { expiry -> expiry > observed } == true }

    private fun granted(permission: String): Boolean = activity.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED

    private fun deviceId(): String = activity.getSharedPreferences("floe_context", Activity.MODE_PRIVATE).let { preferences ->
        preferences.getString("device_id", null) ?: ("android-" + UUID.randomUUID().toString()).also { preferences.edit().putString("device_id", it).apply() }
    }

    private fun opaqueHandle(namespace: String, value: String): String {
        val digest = MessageDigest.getInstance("SHA-256").digest((namespace + "\u0000" + value).toByteArray(StandardCharsets.UTF_8))
        return namespace + ":" + digest.take(16).joinToString("") { "%02x".format(it) }
    }

    private fun requireEncodedLimit(value: Map<String, Any?>, maximum: Int) {
        if (JSONObject(value).toString().toByteArray(StandardCharsets.UTF_8).size > maximum) throw ContextFailure("budget_exceeded", "Android context view exceeds its byte budget.")
    }

    private fun Cursor.requiredString(index: Int, maximum: Int): String {
        val value = getString(index) ?: throw ContextFailure("invalid_response", "Android provider identifier is missing.")
        if (!validOpaque(value, maximum)) throw ContextFailure("invalid_response", "Android provider identifier is invalid.")
        return value
    }

    private fun validOpaque(value: String, maximum: Int): Boolean = value.isNotBlank() && value.length <= maximum && value.none { it == '\u0000' || it == '\r' || it == '\n' }

    private fun Map<String, Any?>.long(name: String): Long = (this[name] as? Number)?.toLong() ?: throw ContextFailure("invalid_input", "$name is required.")
    private fun Map<String, Any?>.int(name: String): Int = (this[name] as? Number)?.toInt() ?: throw ContextFailure("invalid_input", "$name is required.")
    private fun Map<String, Any?>.string(name: String): String = this[name] as? String ?: throw ContextFailure("invalid_input", "$name is required.")
    private fun Map<String, Any?>.stringList(name: String): List<String> = (this[name] as? List<*>)?.map { it as? String ?: throw ContextFailure("invalid_input", "$name is invalid.") } ?: throw ContextFailure("invalid_input", "$name is required.")

    private class ContextFailure(val code: String, override val message: String) : RuntimeException(message)

    private object ContentResolverKeys {
        const val SELECTION = android.content.ContentResolver.QUERY_ARG_SQL_SELECTION
        const val SELECTION_ARGS = android.content.ContentResolver.QUERY_ARG_SQL_SELECTION_ARGS
        const val SORT_COLUMNS = android.content.ContentResolver.QUERY_ARG_SORT_COLUMNS
        const val SORT_DIRECTION = android.content.ContentResolver.QUERY_ARG_SORT_DIRECTION
        const val LIMIT = android.content.ContentResolver.QUERY_ARG_LIMIT
        const val OFFSET = android.content.ContentResolver.QUERY_ARG_OFFSET
    }

    private companion object {
        const val FRESHNESS_MS = 300_000L
        const val MAX_RANGE_MS = 32L * 86_400_000L
        const val MAX_CALENDAR_ITEMS = 128
        const val MAX_CONTACT_ITEMS = 64
        const val MAX_VIEW_BYTES = 65_536
        const val MAX_PEOPLE_BYTES = 32_768
        const val MAX_TITLE_CHARS = 256
        const val MAX_CONTACT_NAME_CHARS = 256
    }
}
