package app.floe.floe_client

import android.Manifest
import android.app.Activity
import android.content.ContentUris
import android.content.Intent
import android.content.pm.PackageManager
import android.database.Cursor
import android.os.Build
import android.os.Bundle
import android.provider.CalendarContract
import android.provider.ContactsContract
import androidx.health.connect.client.HealthConnectClient
import androidx.health.connect.client.PermissionController
import androidx.health.connect.client.permission.HealthPermission
import androidx.health.connect.client.records.ExerciseSessionRecord
import androidx.health.connect.client.records.SleepSessionRecord
import androidx.health.connect.client.records.StepsRecord
import androidx.health.connect.client.request.ReadRecordsRequest
import androidx.health.connect.client.time.TimeRangeFilter
import io.flutter.plugin.common.BinaryMessenger
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.nio.charset.StandardCharsets
import java.security.MessageDigest
import java.time.Duration
import java.time.Instant
import java.util.UUID
import java.util.concurrent.Executors
import kotlinx.coroutines.runBlocking
import org.json.JSONObject

internal class AndroidContextChannel(
    private val activity: Activity,
    messenger: BinaryMessenger,
) : MethodChannel.MethodCallHandler {
    private val channel = MethodChannel(messenger, "floe/android_context")
    private val worker = Executors.newSingleThreadExecutor()
    private var permissionResult: MethodChannel.Result? = null
    private var healthPermissionResult: MethodChannel.Result? = null
    private val healthPermissionContract = PermissionController.createRequestPermissionResultContract()
    private var permissionRequestCode = 7000
    @Volatile private var calendarLastView: Map<String, Any?>? = null
    @Volatile private var contactsLastView: Map<String, Any?>? = null
    @Volatile private var healthLastView: Map<String, Any?>? = null
    @Volatile private var calendarLastSuccess: Long? = null
    @Volatile private var contactsLastSuccess: Long? = null
    private val contactNativeIdentifiersByHandle = mutableMapOf<String, String>()
    @Volatile private var healthLastSuccess: Long? = null

    init {
        channel.setMethodCallHandler(this)
    }

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "connections" -> runWorker(result) { connectionSnapshots() }
            "requestPermission" -> requestPermission(call, result)
            "listCalendars" -> runWorker(result) { listCalendars() }
            "selectedCalendars" -> result.success(selectedCalendarIds())
            "setSelectedCalendars" -> runWorker(result) { setSelectedCalendars(arguments(call)) }
            "readCalendar" -> runWorker(result) { readCalendar(arguments(call)) }
            "readAcquisition" -> runWorker(result) { readAcquisition(arguments(call)) }
            "readContacts" -> runWorker(result) { readContacts(arguments(call)) }
            "inspectContactsSubject" -> runWorker(result) { inspectContactsSubject(arguments(call)) }
            "readWellbeing" -> runWorker(result) { readWellbeing() }
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

    fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?): Boolean {
        if (requestCode != HEALTH_PERMISSION_REQUEST_CODE || healthPermissionResult == null) return false
        val granted = healthPermissionContract.parseResult(resultCode, data)
        val pending = healthPermissionResult
        healthPermissionResult = null
        pending?.success(mapOf("granted" to granted.containsAll(HEALTH_PERMISSIONS)))
        return true
    }

    fun close() {
        channel.setMethodCallHandler(null)
        permissionResult?.error("cancelled", "Permission request was cancelled.", null)
        permissionResult = null
        healthPermissionResult?.error("cancelled", "Permission request was cancelled.", null)
        healthPermissionResult = null
        worker.shutdownNow()
    }

    private fun requestPermission(call: MethodCall, result: MethodChannel.Result) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) {
            result.error("unsupported", "Android runtime permissions are unavailable.", null)
            return
        }
        if (permissionResult != null || healthPermissionResult != null) {
            result.error("busy", "Another permission request is active.", null)
            return
        }
        val source = call.argument<String>("source")
        if (source == "health") {
            if (healthStatus() != HealthConnectClient.SDK_AVAILABLE) {
                result.error("unsupported", "Health Connect is unavailable.", null)
                return
            }
            healthPermissionResult = result
            try {
                activity.startActivityForResult(
                    healthPermissionContract.createIntent(activity, HEALTH_PERMISSIONS),
                    HEALTH_PERMISSION_REQUEST_CODE,
                )
            } catch (_: Throwable) {
                healthPermissionResult = null
                result.error("unavailable", "Health Connect permission UI is unavailable.", null)
            }
            return
        }
        val permissions = when (source) {
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
        val calendarIds = selectedCalendarIds()
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

    private fun listCalendars(): List<Map<String, Any?>> {
        requirePermission(Manifest.permission.READ_CALENDAR)
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) throw ContextFailure("unsupported", "Bounded provider queries require Android 8 or later.")
        val query = Bundle().apply {
            putString(ContentResolverKeys.SELECTION, "${CalendarContract.Calendars.VISIBLE} = ?")
            putStringArray(ContentResolverKeys.SELECTION_ARGS, arrayOf("1"))
            putStringArray(ContentResolverKeys.SORT_COLUMNS, arrayOf(CalendarContract.Calendars.CALENDAR_DISPLAY_NAME))
            putInt(ContentResolverKeys.SORT_DIRECTION, android.content.ContentResolver.QUERY_SORT_DIRECTION_ASCENDING)
            putInt(ContentResolverKeys.LIMIT, MAX_AVAILABLE_CALENDARS + 1)
        }
        val projection = arrayOf(CalendarContract.Calendars._ID, CalendarContract.Calendars.CALENDAR_DISPLAY_NAME)
        val calendars = mutableListOf<Map<String, Any?>>()
        activity.contentResolver.query(CalendarContract.Calendars.CONTENT_URI, projection, query, null)?.use { cursor ->
            while (cursor.moveToNext()) {
                if (calendars.size == MAX_AVAILABLE_CALENDARS) break
                val identifier = cursor.requiredString(0, 512)
                val name = cursor.getString(1)?.trim().orEmpty().take(MAX_CALENDAR_NAME_CHARS)
                if (name.isEmpty()) continue
                calendars += mapOf("calendar_id" to identifier, "display_name" to name)
            }
        } ?: throw ContextFailure("unavailable", "Calendar provider returned no cursor.")
        return calendars
    }

    private fun readAcquisition(arguments: Map<String, Any?>): Map<String, Any?> {
        val requestId = arguments.string("request_id")
        val hostEpoch = arguments.string("host_epoch")
        val personId = arguments.string("person_id")
        val deviceId = arguments.string("device_id")
        val connectionId = arguments.string("connection_id")
        val revision = arguments.int("connection_revision")
        val provider = arguments.string("provider")
        val mode = arguments.string("mode")
        val ids = arguments.stringList("calendar_ids").sorted()
        val start = arguments.long("range_start_unix_ms")
        val end = arguments.long("range_end_unix_ms")
        val deadline = arguments.long("deadline_unix_ms")
        if (provider != "android" || (mode != "inspect_subject" && mode != "read_events") ||
            revision <= 0 || ids.isEmpty() || ids.size > MAX_SELECTED_CALENDARS ||
            ids.toSet().size != ids.size || ids.any { !validOpaque(it, 512) } ||
            start < 0 || end <= start || end - start > MAX_RANGE_MS ||
            deadline <= System.currentTimeMillis() || deadline - System.currentTimeMillis() > MAX_ACQUISITION_DEADLINE_MS) {
            throw ContextFailure("invalid_input", "Calendar acquisition is outside its bounded scope.")
        }
        if (ids != arguments.stringList("calendar_ids")) {
            throw ContextFailure("invalid_input", "Calendar identifiers must be sorted.")
        }
        if (mode == "read_events") requirePermission(Manifest.permission.READ_CALENDAR)
        val before = calendarSubjectEvidence(ids)
        if (mode == "read_events") {
            val expected = arguments.string("expected_native_subject_fingerprint")
            if (expected != before.fingerprint) throw ContextFailure("stale_context", "Calendar subject changed before acquisition.")
        } else if (arguments["expected_native_subject_fingerprint"] != null) {
            throw ContextFailure("invalid_input", "Subject inspection cannot carry a read fingerprint.")
        }
        val batches = mutableListOf<Map<String, Any?>>()
        var total = 0
        if (mode == "read_events") {
            val uriBase = CalendarContract.Instances.CONTENT_URI
            for (calendarId in ids) {
                if (deadline <= System.currentTimeMillis()) throw ContextFailure("deadline_exceeded", "Calendar acquisition deadline elapsed.")
                val uriBuilder = uriBase.buildUpon()
                ContentUris.appendId(uriBuilder, start)
                ContentUris.appendId(uriBuilder, end)
                val query = Bundle().apply {
                    putString(ContentResolverKeys.SELECTION, "${CalendarContract.Instances.CALENDAR_ID} = ?")
                    putStringArray(ContentResolverKeys.SELECTION_ARGS, arrayOf(calendarId))
                    putStringArray(ContentResolverKeys.SORT_COLUMNS, arrayOf(CalendarContract.Instances.BEGIN))
                    putInt(ContentResolverKeys.SORT_DIRECTION, android.content.ContentResolver.QUERY_SORT_DIRECTION_ASCENDING)
                    putInt(ContentResolverKeys.LIMIT, MAX_CALENDAR_ITEMS - total + 1)
                }
                val projection = arrayOf(
                    CalendarContract.Instances.EVENT_ID,
                    CalendarContract.Instances.CALENDAR_ID,
                    CalendarContract.Instances.TITLE,
                    CalendarContract.Instances.BEGIN,
                    CalendarContract.Instances.END,
                    CalendarContract.Instances.ALL_DAY,
                )
                val records = mutableListOf<Map<String, Any?>>()
                activity.contentResolver.query(uriBuilder.build(), projection, query, null)?.use { cursor ->
                    while (cursor.moveToNext()) {
                        if (total >= MAX_CALENDAR_ITEMS) throw ContextFailure("budget_exceeded", "Calendar acquisition item budget exceeded.")
                        total += 1
                        records += acquisitionRecord(cursor)
                        val partial = mapOf("calendar_id" to calendarId, "records" to records, "failure" to null)
                        if (JSONObject(mapOf("batches" to batches + partial)).toString().toByteArray(StandardCharsets.UTF_8).size > MAX_ACQUISITION_BYTES) {
                            throw ContextFailure("budget_exceeded", "Calendar acquisition byte budget exceeded.")
                        }
                    }
                } ?: throw ContextFailure("unavailable", "Calendar provider returned no cursor.")
                batches += mapOf("calendar_id" to calendarId, "records" to records, "failure" to null)
            }
        }
        val after = calendarSubjectEvidence(ids)
        if (after.fingerprint != before.fingerprint || deadline <= System.currentTimeMillis()) {
            throw ContextFailure("stale_context", "Calendar subject changed during acquisition.")
        }
        val response = linkedMapOf<String, Any?>(
            "request_id" to requestId,
            "host_epoch" to hostEpoch,
            "person_id" to personId,
            "device_id" to deviceId,
            "connection_id" to connectionId,
            "connection_revision" to revision,
            "provider" to provider,
            "mode" to mode,
            "calendar_ids" to ids,
            "range_start_unix_ms" to start,
            "range_end_unix_ms" to end,
            "native_subject_fingerprint_before" to before.fingerprint,
            "native_subject_fingerprint_after" to after.fingerprint,
            "available_calendar_ids" to before.availableCalendarIds,
            "permission_class" to before.permissionClass,
            "batches" to batches,
        )
        if (JSONObject(response).toString().toByteArray(StandardCharsets.UTF_8).size > MAX_ACQUISITION_BYTES) {
            throw ContextFailure("budget_exceeded", "Calendar acquisition byte budget exceeded.")
        }
        return response
    }

    private fun calendarSubjectEvidence(selected: List<String>): CalendarSubjectEvidence {
        val permissionClass = if (granted(Manifest.permission.READ_CALENDAR)) "read_calendar" else "denied"
        val projection = arrayOf(
            CalendarContract.Calendars._ID,
            CalendarContract.Calendars.ACCOUNT_NAME,
            CalendarContract.Calendars.ACCOUNT_TYPE,
            CalendarContract.Calendars.OWNER_ACCOUNT,
        )
        val rows = mutableListOf<Map<String, String>>()
        activity.contentResolver.query(CalendarContract.Calendars.CONTENT_URI, projection, null, null)?.use { cursor ->
            while (cursor.moveToNext() && rows.size < MAX_AVAILABLE_CALENDARS) {
                val id = cursor.requiredString(0, 512)
                rows += mapOf(
                    "calendar_id" to id,
                    "account_name" to (cursor.getString(1) ?: ""),
                    "account_type" to (cursor.getString(2) ?: ""),
                    "owner_account" to (cursor.getString(3) ?: ""),
                )
            }
        } ?: throw ContextFailure("unavailable", "Calendar provider returned no cursor.")
        val available = rows.map { it.getValue("calendar_id") }.sorted()
        val selectedRows = rows.filter { selected.contains(it.getValue("calendar_id")) }.sortedBy { it.getValue("calendar_id") }
        if (selectedRows.size != selected.size) throw ContextFailure("calendar_unavailable", "Calendar selection is unavailable.")
        val canonical = JSONObject().apply {
            put("permission", permissionClass)
            put("calendars", selectedRows.map { JSONObject(it) })
        }.toString()
        val digest = MessageDigest.getInstance("SHA-256").digest(canonical.toByteArray(StandardCharsets.UTF_8))
            .joinToString("") { "%02x".format(it) }
        return CalendarSubjectEvidence(digest, available, permissionClass)
    }

    private fun acquisitionRecord(cursor: Cursor): Map<String, Any?> {
        val eventId = cursor.requiredString(0, 512)
        val calendarId = cursor.requiredString(1, 512)
        val title = cursor.getString(2)?.trim()?.take(MAX_TITLE_CHARS) ?: ""
        val starts = cursor.getLong(3)
        val ends = cursor.getLong(4)
        if (starts < 0 || ends <= starts) throw ContextFailure("invalid_response", "Calendar provider returned an invalid event.")
        val schedule = mapOf(
            "kind" to "timed",
            "starts_at" to Instant.ofEpochMilli(starts).toString(),
            "ends_at" to Instant.ofEpochMilli(ends).toString(),
            "timezone" to "UTC",
        )
        return mapOf(
            "can_modify" to false,
            "calendar_id" to calendarId,
            "external_id" to eventId,
            "external_revision" to "$starts:$ends:${cursor.getInt(5)}",
            "title" to title,
            "schedule" to schedule,
        )
    }

    private fun setSelectedCalendars(arguments: Map<String, Any?>): Map<String, Any?> {
        val requested = arguments.stringList("calendar_ids")
        if (requested.size > MAX_SELECTED_CALENDARS || requested.toSet().size != requested.size || requested.any { !validOpaque(it, 512) }) {
            throw ContextFailure("invalid_input", "Calendar selection is invalid.")
        }
        val available = listCalendars().map { it["calendar_id"] as String }.toSet()
        if (!available.containsAll(requested)) throw ContextFailure("invalid_input", "Calendar selection is unavailable.")
        val canonical = requested.sorted()
        if (canonical != selectedCalendarIds()) {
            activity.getSharedPreferences("floe_context", Activity.MODE_PRIVATE).edit().putStringSet(CALENDAR_SELECTION_KEY, canonical.toSet()).apply()
            calendarLastView = null
            calendarLastSuccess = null
        }
        return mapOf("calendar_ids" to canonical)
    }

    private fun selectedCalendarIds(): List<String> = activity.getSharedPreferences("floe_context", Activity.MODE_PRIVATE)
        .getStringSet(CALENDAR_SELECTION_KEY, emptySet())
        .orEmpty()
        .filter { validOpaque(it, 512) }
        .distinct()
        .sorted()
        .take(MAX_SELECTED_CALENDARS)

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
        val selectedHandles = (arguments["selected_handles"] as? List<*>)?.map {
            it as? String ?: throw ContextFailure("invalid_input", "selected_handles is invalid.")
        }
        if (selectedHandles != null && (selectedHandles.isEmpty() || selectedHandles.size > MAX_CONTACT_ITEMS || selectedHandles.toSet().size != selectedHandles.size || selectedHandles.any { !validOpaque(it, 512) })) {
            throw ContextFailure("invalid_input", "selected_handles is invalid.")
        }
        val selectedIdentifiers = selectedHandles?.map {
            contactNativeIdentifiersByHandle[it] ?: throw ContextFailure("unavailable", "Selected contact is unavailable.")
        }
        val query = Bundle().apply {
            putStringArray(ContentResolverKeys.SORT_COLUMNS, arrayOf(ContactsContract.Contacts.DISPLAY_NAME_PRIMARY))
            putInt(ContentResolverKeys.SORT_DIRECTION, android.content.ContentResolver.QUERY_SORT_DIRECTION_ASCENDING)
            putInt(ContentResolverKeys.LIMIT, limit + 1)
            if (selectedIdentifiers != null) {
                putString(ContentResolverKeys.SELECTION, selectedIdentifiers.joinToString(",", prefix = "${ContactsContract.Contacts._ID} IN (", postfix = ")") { "?" })
                putStringArray(ContentResolverKeys.SELECTION_ARGS, selectedIdentifiers.toTypedArray())
            }
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
                val identityHandle = opaqueHandle("person.identity", identifier)
                contactNativeIdentifiersByHandle[identityHandle] = identifier
                identities += linkedMapOf(
                    "identity_handle" to identityHandle,
                    "display_name" to name,
                    "aliases" to emptyList<String>(),
                    "confidence_millis" to 1000,
                    "evidence_handles" to listOf(evidence),
                )
            }
        } ?: throw ContextFailure("unavailable", "Contacts provider returned no cursor.")
        requirePermission(Manifest.permission.READ_CONTACTS)
        val observed = System.currentTimeMillis()
        val selectedResolved = selectedHandles?.all { handle -> identities.any { it["identity_handle"] == handle } } ?: true
        val view = linkedMapOf<String, Any?>(
            "schema_version" to 1,
            "view_id" to "people.identity",
            "source_handle" to opaqueHandle("people", deviceId()),
            "observed_at_unix_ms" to observed,
            "expires_at_unix_ms" to observed + FRESHNESS_MS,
            "coverage_complete" to (!hasMore && selectedResolved),
            "identities" to identities,
        )
        requireEncodedLimit(view, MAX_PEOPLE_BYTES)
        contactsLastView = view
        contactsLastSuccess = observed
        return view
    }

    private fun inspectContactsSubject(arguments: Map<String, Any?>): Map<String, Any?> {
        if (arguments.keys != setOf("selected_handles")) {
            throw ContextFailure("invalid_input", "Invalid Contacts subject request.")
        }
        requirePermission(Manifest.permission.READ_CONTACTS)
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            throw ContextFailure("unsupported", "Bounded provider queries require Android 8 or later.")
        }
        val selectedHandles = (arguments["selected_handles"] as? List<*>)?.map {
            it as? String ?: throw ContextFailure("invalid_input", "selected_handles is invalid.")
        } ?: throw ContextFailure("invalid_input", "selected_handles is required.")
        if (selectedHandles.isEmpty() || selectedHandles.size > MAX_CONTACT_ITEMS ||
            selectedHandles.toSet().size != selectedHandles.size ||
            selectedHandles.any { !validOpaque(it, 512) }) {
            throw ContextFailure("invalid_input", "selected_handles is invalid.")
        }
        val identifiers = selectedHandles.map {
            contactNativeIdentifiersByHandle[it]
                ?: throw ContextFailure("unavailable", "Selected contact is unavailable.")
        }
        val query = Bundle().apply {
            putString(
                ContentResolverKeys.SELECTION,
                identifiers.joinToString(",", prefix = "${ContactsContract.Contacts._ID} IN (", postfix = ")") { "?" },
            )
            putStringArray(ContentResolverKeys.SELECTION_ARGS, identifiers.toTypedArray())
            putInt(ContentResolverKeys.LIMIT, identifiers.size + 1)
        }
        val resolved = mutableSetOf<String>()
        val projection = arrayOf(ContactsContract.Contacts._ID)
        activity.contentResolver.query(ContactsContract.Contacts.CONTENT_URI, projection, query, null)?.use { cursor ->
            while (cursor.moveToNext()) resolved += cursor.requiredString(0, 512)
        } ?: throw ContextFailure("unavailable", "Contacts provider returned no cursor.")
        requirePermission(Manifest.permission.READ_CONTACTS)
        if (resolved != identifiers.toSet()) {
            throw ContextFailure("unavailable", "Selected contact is unavailable.")
        }
        val permissionClass = "read_contacts"
        val fingerprint = nativeFingerprint(permissionClass, identifiers)
        return linkedMapOf(
            "schema_version" to 1,
            "subject_fingerprint" to fingerprint,
            "permission_class" to permissionClass,
            "resolved_handles" to selectedHandles.sorted(),
        )
    }

    private fun nativeFingerprint(permissionClass: String, identifiers: List<String>): String {
        val input = "contacts.subject\u0000$permissionClass\u0000${identifiers.sorted().joinToString("\u0000")}"
        return MessageDigest.getInstance("SHA-256")
            .digest(input.toByteArray(StandardCharsets.UTF_8))
            .joinToString("") { "%02x".format(it) }
    }

    private fun readWellbeing(): Map<String, Any?> {
        val client = healthClient()
        val granted = runBlocking { client.permissionController.getGrantedPermissions() }
        if (!granted.containsAll(HEALTH_PERMISSIONS)) {
            healthLastView = null
            throw ContextFailure("permission_denied", "Health Connect permission was denied.")
        }
        val end = Instant.now()
        val start = end.minus(Duration.ofHours(36))
        val filter = TimeRangeFilter.between(start, end)
        val sleep = runBlocking { client.readRecords(healthRequest(SleepSessionRecord::class, filter)) }
        val steps = runBlocking { client.readRecords(healthRequest(StepsRecord::class, filter)) }
        val exercise = runBlocking { client.readRecords(healthRequest(ExerciseSessionRecord::class, filter)) }
        val sleepHours = sleep.records.sumOf { Duration.between(it.startTime, it.endTime).toMinutes().coerceAtLeast(0) } / 60.0
        val stepCount = steps.records.sumOf { it.count.coerceAtLeast(0) }
        val exerciseMinutes = exercise.records.sumOf { Duration.between(it.startTime, it.endTime).toMinutes().coerceAtLeast(0) }
        val hasEvidence = sleep.records.isNotEmpty() || steps.records.isNotEmpty() || exercise.records.isNotEmpty()
        val partial = sleep.pageToken != null || steps.pageToken != null || exercise.pageToken != null
        val capacity = when {
            !hasEvidence -> "unknown"
            sleep.records.isNotEmpty() && sleepHours < 6 -> "reduced"
            sleepHours >= 8 && stepCount >= 8_000 -> "strong"
            else -> "typical"
        }
        val recovery = when {
            !hasEvidence -> "unknown"
            sleep.records.isNotEmpty() && sleepHours < 6 -> "needs_recovery"
            sleepHours >= 8 -> "recovered"
            else -> "typical"
        }
        val evidence = buildList {
            if (sleep.records.isNotEmpty()) add(opaqueHandle("health.sleep.window", "$start\u0000$end\u0000${sleep.records.size}"))
            if (steps.records.isNotEmpty()) add(opaqueHandle("health.steps.window", "$start\u0000$end\u0000${steps.records.size}"))
            if (exercise.records.isNotEmpty()) add(opaqueHandle("health.exercise.window", "$start\u0000$end\u0000${exercise.records.size}\u0000$exerciseMinutes"))
        }
        val observed = System.currentTimeMillis()
        val view = linkedMapOf<String, Any?>(
            "schema_version" to 1,
            "view_id" to "wellbeing.derived",
            "source_handle" to opaqueHandle("wellbeing", deviceId()),
            "observed_at_unix_ms" to observed,
            "expires_at_unix_ms" to observed + FRESHNESS_MS,
            "capacity" to capacity,
            "recovery" to recovery,
            "confidence_millis" to if (!hasEvidence) 0 else if (partial) 500 else 700,
            "evidence_handles" to evidence,
        )
        requireEncodedLimit(view, MAX_WELLBEING_BYTES)
        healthLastView = view
        healthLastSuccess = observed
        return view
    }

    @OptIn(androidx.health.connect.client.ExperimentalDeduplicationApi::class)
    private fun <T : androidx.health.connect.client.records.Record> healthRequest(
        recordType: kotlin.reflect.KClass<T>,
        filter: TimeRangeFilter,
    ): ReadRecordsRequest<T> = ReadRecordsRequest(
        recordType = recordType,
        timeRangeFilter = filter,
        pageSize = MAX_HEALTH_RECORDS,
        deduplicateStrategy = HEALTH_DEDUPLICATION_STRATEGY,
    )

    private fun connectionSnapshots(): List<Map<String, Any?>> {
        val observed = System.currentTimeMillis()
        val calendarConfigured = selectedCalendarIds().isNotEmpty()
        calendarLastView = if (granted(Manifest.permission.READ_CALENDAR) && calendarConfigured) freshView(calendarLastView, observed) else null
        contactsLastView = if (granted(Manifest.permission.READ_CONTACTS)) freshView(contactsLastView, observed) else null
        val healthStatus = healthStatus()
        val healthGranted = if (healthStatus == HealthConnectClient.SDK_AVAILABLE) healthGrantedPermissions() else emptySet()
        healthLastView = if (healthGranted?.containsAll(HEALTH_PERMISSIONS) == true) freshView(healthLastView, observed) else null
        return listOf(
            connectionSnapshot("calendar.android", "android_calendar", "calendar.events.read", Manifest.permission.READ_CALENDAR, "calendar.timeline", 128, 65_536, calendarLastView, calendarLastSuccess, "items", observed, configured = calendarConfigured),
            connectionSnapshot("contacts.android", "android_contacts", "contacts.identity.read", Manifest.permission.READ_CONTACTS, "people.identity", 64, 32_768, contactsLastView, contactsLastSuccess, "identities", observed),
            healthConnectionSnapshot(healthStatus, healthGranted, observed),
        )
    }

    private fun healthConnectionSnapshot(status: Int, granted: Set<String>?, observed: Long): Map<String, Any?> {
        val available = status == HealthConnectClient.SDK_AVAILABLE
        val allowed = available && granted?.containsAll(HEALTH_PERMISSIONS) == true
        val lastObserved = healthLastView?.get("observed_at_unix_ms") as? Long
        val lastExpires = healthLastView?.get("expires_at_unix_ms") as? Long
        val fresh = allowed && lastObserved != null && lastExpires != null && lastExpires > observed
        val state = when {
            !available -> "unsupported"
            granted == null -> "unavailable"
            !allowed -> "revoked"
            healthLastView == null && healthLastSuccess == null -> "pending"
            fresh -> "ready"
            else -> "unavailable"
        }
        val connection = linkedMapOf<String, Any?>(
            "schema_version" to 1,
            "connector_id" to "health.android",
            "state" to state,
            "granted_scopes" to (granted?.intersect(HEALTH_PERMISSIONS)?.sorted() ?: emptyList<String>()),
            "observed_at_unix_ms" to observed,
        )
        if (healthLastSuccess != null) connection["last_success_at_unix_ms"] = healthLastSuccess
        if (!available) connection["last_failure"] = mapOf("kind" to "unsupported_entitlement", "observed_at_unix_ms" to observed)
        else if (granted == null) connection["last_failure"] = mapOf("kind" to "unavailable", "observed_at_unix_ms" to observed)
        else if (!allowed) connection["last_failure"] = mapOf("kind" to "permission_denied", "observed_at_unix_ms" to observed)
        else if (!fresh && healthLastSuccess != null) connection["last_failure"] = mapOf("kind" to "stale", "observed_at_unix_ms" to observed)
        val freshHealthView = healthLastView
        val views = if (fresh && freshHealthView != null) listOf(mapOf(
            "schema_version" to 1,
            "view_id" to "wellbeing.derived",
            "source_handle" to freshHealthView["source_handle"],
            "observed_at_unix_ms" to lastObserved,
            "expires_at_unix_ms" to lastExpires,
            "item_count" to if ((freshHealthView["evidence_handles"] as? List<*>)?.isEmpty() == false) 1 else 0,
            "byte_count" to JSONObject(freshHealthView).toString().toByteArray(StandardCharsets.UTF_8).size,
            "provenance_count" to ((freshHealthView["evidence_handles"] as? List<*>)?.size ?: 0),
        )) else emptyList()
        return mapOf(
            "descriptor" to mapOf(
                "schema_version" to 1,
                "id" to "health.android",
                "version" to "1.0.0",
                "provider" to "health_connect",
                "execution" to mapOf("kind" to "device", "device_id" to deviceId()),
                "capabilities" to listOf(mapOf("schema_version" to 1, "id" to "health.derived.read", "version" to "1.0.0", "authority" to "observe", "required_scopes" to HEALTH_PERMISSIONS.sorted(), "output_view_id" to "wellbeing.derived")),
                "views" to listOf(mapOf("schema_version" to 1, "id" to "wellbeing.derived", "version" to "1.0.0", "data_class" to "personal", "retention" to "derived_only", "freshness_ttl_ms" to FRESHNESS_MS, "max_items" to 1, "max_bytes" to MAX_WELLBEING_BYTES, "provenance_required" to true)),
            ),
            "connection" to connection,
            "views" to views,
        )
    }

    private fun connectionSnapshot(connector: String, provider: String, capability: String, permission: String, viewId: String, maxItems: Int, maxBytes: Int, lastView: Map<String, Any?>?, lastSuccess: Long?, itemsKey: String, observed: Long, configured: Boolean = true): Map<String, Any?> {
        val allowed = granted(permission)
        val lastObserved = lastView?.get("observed_at_unix_ms") as? Long
        val lastExpires = lastView?.get("expires_at_unix_ms") as? Long
        val fresh = allowed && lastObserved != null && lastExpires != null && lastExpires > observed
        val connection = linkedMapOf<String, Any?>(
            "schema_version" to 1,
            "connector_id" to connector,
            "state" to when {
                !allowed -> "revoked"
                !configured -> "pending"
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
            val items = lastView[itemsKey] as? List<*> ?: emptyList<Any>()
            listOf(mapOf(
                "schema_version" to 1,
                "view_id" to viewId,
                "source_handle" to lastView["source_handle"],
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

    private fun healthStatus(): Int = try {
        HealthConnectClient.getSdkStatus(activity)
    } catch (_: Throwable) {
        HealthConnectClient.SDK_UNAVAILABLE
    }

    private fun healthClient(): HealthConnectClient {
        if (healthStatus() != HealthConnectClient.SDK_AVAILABLE) {
            healthLastView = null
            throw ContextFailure("unsupported", "Health Connect is unavailable.")
        }
        return HealthConnectClient.getOrCreate(activity)
    }

    private fun healthGrantedPermissions(): Set<String>? = try {
        runBlocking { HealthConnectClient.getOrCreate(activity).permissionController.getGrantedPermissions() }
    } catch (_: Throwable) {
        null
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

    private data class CalendarSubjectEvidence(
        val fingerprint: String,
        val availableCalendarIds: List<String>,
        val permissionClass: String,
    )

    private object ContentResolverKeys {
        const val SELECTION = android.content.ContentResolver.QUERY_ARG_SQL_SELECTION
        const val SELECTION_ARGS = android.content.ContentResolver.QUERY_ARG_SQL_SELECTION_ARGS
        const val SORT_COLUMNS = android.content.ContentResolver.QUERY_ARG_SORT_COLUMNS
        const val SORT_DIRECTION = android.content.ContentResolver.QUERY_ARG_SORT_DIRECTION
        const val LIMIT = android.content.ContentResolver.QUERY_ARG_LIMIT
        const val OFFSET = android.content.ContentResolver.QUERY_ARG_OFFSET
    }

    private companion object {
        val HEALTH_PERMISSIONS = setOf(
            HealthPermission.getReadPermission(SleepSessionRecord::class),
            HealthPermission.getReadPermission(StepsRecord::class),
            HealthPermission.getReadPermission(ExerciseSessionRecord::class),
        )
        const val FRESHNESS_MS = 300_000L
        const val MAX_RANGE_MS = 32L * 86_400_000L
        const val MAX_CALENDAR_ITEMS = 128
        const val MAX_ACQUISITION_BYTES = 65_536
        const val MAX_ACQUISITION_DEADLINE_MS = 30_000L
        const val MAX_AVAILABLE_CALENDARS = 32
        const val MAX_SELECTED_CALENDARS = 4
        const val MAX_CONTACT_ITEMS = 64
        const val MAX_HEALTH_RECORDS = 100
        const val HEALTH_DEDUPLICATION_STRATEGY = 1
        const val HEALTH_PERMISSION_REQUEST_CODE = 8000
        const val MAX_VIEW_BYTES = 65_536
        const val MAX_PEOPLE_BYTES = 32_768
        const val MAX_WELLBEING_BYTES = 32_768
        const val MAX_TITLE_CHARS = 256
        const val MAX_CALENDAR_NAME_CHARS = 256
        const val MAX_CONTACT_NAME_CHARS = 256
        const val CALENDAR_SELECTION_KEY = "selected_calendar_ids"
    }
}
