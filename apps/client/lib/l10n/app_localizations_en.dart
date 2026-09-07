// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get agentEntry => 'Floe is here to help';

  @override
  String get agentEntryHint => 'Try a sample conversation';

  @override
  String get agentSampleTitle => 'Sample conversation';

  @override
  String get agentSampleBoundary =>
      'Sample data only. Personal chat stays locked until secure storage is ready.';

  @override
  String get agentNewConversation => 'New sample conversation';

  @override
  String get agentEmpty =>
      'Start with a sample briefing, then ask a follow-up. No connected sources are read.';

  @override
  String get agentPrompt => 'Sample question';

  @override
  String get agentBriefing => 'Show a sample briefing';

  @override
  String get agentFollowUp => 'What can Floe change?';

  @override
  String get agentSend => 'Send sample';

  @override
  String get agentStop => 'Stop response';

  @override
  String get agentRetry => 'Try again';

  @override
  String get agentReload => 'Reload conversation';

  @override
  String get agentRecover => 'Recover conversation';

  @override
  String get agentYou => 'You';

  @override
  String get agentSource => 'Sample schedule';

  @override
  String get agentSourceDetails => 'View sample source';

  @override
  String get agentSourceUnavailable => 'This sample source is unavailable.';

  @override
  String get agentLoading => 'Loading saved conversation…';

  @override
  String get agentPreparing => 'Preparing sample reply…';

  @override
  String get agentReading => 'Reading the sample schedule…';

  @override
  String get agentStopping => 'Stopping response…';

  @override
  String get agentStopped => 'Response stopped. Saved messages are kept.';

  @override
  String get agentInterrupted =>
      'The previous response did not finish. Recover the saved conversation before continuing.';

  @override
  String get agentRecovered =>
      'Saved conversation recovered. You can try the question again.';

  @override
  String get agentUnavailable =>
      'The sample model is unavailable. Your saved conversation is kept.';

  @override
  String get agentBudget =>
      'This response reached its limit. You can ask another sample question.';

  @override
  String get agentStalled =>
      'The sample read repeated without progress, so Floe stopped it.';

  @override
  String get agentReloadNeeded =>
      'The response could not be confirmed. Reload saved messages before sending again.';

  @override
  String get agentFailure =>
      'The response could not finish. Saved messages are kept.';

  @override
  String get settings => 'Settings';

  @override
  String get calendarCouldNotBeCollectedCheckAccess =>
      'Calendar could not be collected. Check access and try again.';

  @override
  String get showingSavedEventsCalendarChangesCouldNot =>
      'Showing saved events. Calendar changes could not be collected.';

  @override
  String get manageConnection => 'Manage connection';

  @override
  String get taskCompleted => 'Task completed.';

  @override
  String get taskMarkedIncomplete => 'Task marked incomplete.';

  @override
  String get undo => 'Undo';

  @override
  String get today => 'Today';

  @override
  String get goToToday => 'Go to today';

  @override
  String get tasks => 'Tasks';

  @override
  String get notes => 'Notes';

  @override
  String get connect => 'Connect';

  @override
  String get previousDay => 'Previous day';

  @override
  String get nextDay => 'Next day';

  @override
  String get refreshCalendar => 'Refresh calendar';

  @override
  String get task => 'Task';

  @override
  String get newTask => 'New task';

  @override
  String get noTasksYet => 'No tasks yet.';

  @override
  String get personal => 'Personal';

  @override
  String get searchNotes => 'Search notes';

  @override
  String get filter => 'Filter';

  @override
  String get newNote => 'New note';

  @override
  String get noNotesFound => 'No notes found';

  @override
  String get tryADifferentSearchOrClearThe =>
      'Try a different search or clear the current filter.';

  @override
  String get clearFilters => 'Clear filters';

  @override
  String get writeAThoughtDecisionOrDetailTo =>
      'Write a thought, decision, or detail to remember.';

  @override
  String get couldNotSavePleaseTryAgain => 'Could not save. Please try again.';

  @override
  String get cancel => 'Cancel';

  @override
  String get saving => 'Saving…';

  @override
  String get saveNote => 'Save note';

  @override
  String get noDescriptionYet => 'No description yet.';

  @override
  String get due => 'Due';

  @override
  String get noDueDate => 'No due date';

  @override
  String get timeContext => 'Time context';

  @override
  String get notScheduled => 'Not scheduled';

  @override
  String get calendar => 'Calendar';

  @override
  String get subtasks => 'Subtasks';

  @override
  String get noSubtasksYet => 'No subtasks yet.';

  @override
  String get addASubtask => 'Add a subtask';

  @override
  String get floeSuggests => 'Floe suggests';

  @override
  String get dismissSuggestion => 'Dismiss suggestion';

  @override
  String get reviewTheContextBeforeStartingThisTask =>
      'Review the context before starting this task?';

  @override
  String get reviewNow => 'Review now';

  @override
  String get snooze => 'Snooze';

  @override
  String get noLinkedNotes => 'No linked notes.';

  @override
  String get updatedThisMorning => 'Updated this morning';

  @override
  String get taskOptions => 'Task options';

  @override
  String get markIncomplete => 'Mark incomplete';

  @override
  String get completeTask => 'Complete task';

  @override
  String get personalNote => 'Personal note';

  @override
  String get close => 'Close';

  @override
  String get thisIsYourOriginalNoteEditingIs =>
      'This is your original note. Editing is not available yet.';

  @override
  String get reviewWithFloe => 'Review with Floe';

  @override
  String get timeNotSet => 'Time not set';

  @override
  String get todaySThought => 'Today\'s thought';

  @override
  String get event => 'Event';

  @override
  String get note => 'Note';

  @override
  String get readOnlyEventManagedInItsOriginal =>
      'Read-only event managed in its original calendar';

  @override
  String get deleteThisItem => 'Delete this item?';

  @override
  String get delete => 'Delete';

  @override
  String get dismissCapture => 'Dismiss capture';

  @override
  String get aThoughtForYourDay => 'A thought for your day...';

  @override
  String get saveCapture => 'Save capture';

  @override
  String get couldNotLoadYourDay => 'Could not load your day';

  @override
  String get anUnknownErrorOccurred => 'An unknown error occurred.';

  @override
  String get tryAgain => 'Try again';

  @override
  String get whereShouldThisGo => 'Where should this go?';

  @override
  String get originalInput => 'Original input';

  @override
  String get thought => 'Thought';

  @override
  String get title => 'Title';

  @override
  String get content => 'Content';

  @override
  String get later => 'Later';

  @override
  String get classifyAndAdd => 'Classify and add';

  @override
  String get pleaseEnterSomeText => 'Please enter some text.';

  @override
  String get endTimeMustBeAfterStartTime =>
      'End time must be after start time.';

  @override
  String get start => 'Start';

  @override
  String get end => 'End';

  @override
  String get setADueDate => 'Set a due date';

  @override
  String get thisFeatureIsNotAvailableYet =>
      'This feature is not available yet.';

  @override
  String get allDay => 'All day';

  @override
  String get zoomOut => 'Zoom out';

  @override
  String get zoomIn => 'Zoom in';

  @override
  String get aLittleBreathingRoom => 'A little breathing room.';

  @override
  String get yourDayIsStillEmpty => 'Your day is still empty.';

  @override
  String get emptyDayCreateHint =>
      'No events on this day. Double-click a time to create one.';

  @override
  String get emptyDayCreateHintTouch =>
      'No events on this day. Use + or press and hold a time to create one.';

  @override
  String get emptyDayConnectHint =>
      'Connect a calendar to create and collect events here.';

  @override
  String get createEvent => 'Create event';

  @override
  String get newEvent => 'New event';

  @override
  String get noSavedEventsForThisDay => 'No saved events for this day.';

  @override
  String get viewConnectedCalendars => 'View connected calendars';

  @override
  String get savedInFloe => 'Saved in Floe';

  @override
  String get calendarsRefreshed => 'Calendars refreshed';

  @override
  String get localTasksAndNotesUnchanged =>
      'Your local tasks and notes are unchanged.';

  @override
  String get localTime => 'Local time';

  @override
  String get lastCollected => 'Last collected';

  @override
  String get allDayBoundary => 'All-day boundary';

  @override
  String get manageThisEventInItsOriginalCalendar =>
      'Manage this event in its original calendar. Floe has no edit or delete action for imported events.';

  @override
  String get sourceDetails => 'Source details';

  @override
  String get connectionPerson => 'Connection / Person';

  @override
  String get externalOccurrenceId => 'External occurrence ID';

  @override
  String get revision => 'Revision';

  @override
  String get integration => 'Integration';

  @override
  String get couldNotConnectOrCollectEventsCheck =>
      'Could not connect or collect events. Check calendar access and try again.';

  @override
  String get connectCalendar => 'Connect Calendar';

  @override
  String get eventsFromTheSelectedCalendarAreSaved =>
      'Events from the selected calendars are saved on this device. Reading never changes Calendar. Creating an event follows your Floe action permission and fresh safety checks. Deselecting a calendar removes its local copy.';

  @override
  String get continueAction => 'Continue';

  @override
  String get noCalendarsAreAvailableAddACalendar =>
      'No calendars are available. Add a calendar in macOS Calendar first.';

  @override
  String get chooseACalendar => 'Choose calendars';

  @override
  String get allCalendarsIncludingNew => 'All calendars, including new ones';

  @override
  String get selectedCalendarsOnly => 'Only selected calendars';

  @override
  String get calendarScope => 'Calendar scope';

  @override
  String get disconnectCalendar => 'Disconnect from Floe';

  @override
  String get disconnectCalendarExplanation =>
      'Remove imported Calendar copies from this Mac? Your local tasks, notes and events, and all external calendars stay unchanged. Reconnect to read them again. OS permission is managed separately.';

  @override
  String get calendarAccessWasDeniedOrRevokedAllow =>
      'Calendar access was denied or revoked. Allow access in Settings, then try again.';

  @override
  String get theSelectedCalendarIsUnavailablePleaseReconnect =>
      'The selected calendar is unavailable. Please reconnect.';

  @override
  String get couldNotCollectEventsShowingTheLast =>
      'Could not collect events. Showing the last saved data.';

  @override
  String get notCollectedYet => 'Not collected yet';

  @override
  String get macosCalendar => 'macOS Calendar';

  @override
  String get calendarsAlreadyOnThisMac => 'Calendars already on this Mac';

  @override
  String get bringYourCalendarIntoOneDayFloe =>
      'Bring your calendar into one day. Connecting only reads events. Calendar creation follows your Floe action permission; editing and deletion are not available.';

  @override
  String get connectedCalendar => 'Connected calendars';

  @override
  String connectedCalendarCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count calendars',
      one: '1 calendar',
    );
    return '$_temp0';
  }

  @override
  String get calendarAccountFallback => 'Other calendars';

  @override
  String get makeRoomForYourDay => 'Make room for your day.';

  @override
  String get chooseACalendarToStartThisClient =>
      'Choose one or more calendars to bring their events into your day.';

  @override
  String get person => 'Person';

  @override
  String get youThisDevice => 'You · this device';

  @override
  String get storedRangeLabel => 'Stored range';

  @override
  String storedRange(String start, String end) {
    return '$start – $end (exclusive)';
  }

  @override
  String get lastSuccessfulRead => 'Last successful read';

  @override
  String get refreshSelectedDay => 'Refresh selected day';

  @override
  String get reconnectOrChange => 'Reconnect or change';

  @override
  String get manageAccess => 'Manage access';

  @override
  String get yourOwnRhythm => 'Your own rhythm';

  @override
  String get noTasksForSelectedDay => 'No tasks for this day.';

  @override
  String get seeYourTasks => 'See your tasks';

  @override
  String get aNoteToSelf => 'A note to self';

  @override
  String get leaveALittleRoomBetweenThingsNot =>
      'Leave a little room between things. Not every empty space needs filling.';

  @override
  String get wonderingWhereAnEventCameFromOpen =>
      'Wondering where an event came from?\nOpen it to see its source calendar.';

  @override
  String get backToConnections => 'Back to connections';

  @override
  String get calendarIntegrationIsUnavailableInThisPreview =>
      'Calendar integration is unavailable in this preview. Use the native macOS client to connect.';

  @override
  String get aClearBoundary => 'A clear boundary.';

  @override
  String get eventsAreSavedOnThisMacFloe =>
      'Reading saves events on this Mac without changing Calendar. Creating an event follows your Floe action permission and fresh safety checks. Editing and deletion are not available.';

  @override
  String get macosCallsThisFullAccessEvenFor =>
      'macOS calls this “Full Access,” even for reading. OS permission alone never authorizes a write: your Floe action permission and fresh safety checks are also required.';

  @override
  String get whatHappensOffline => 'What happens offline?';

  @override
  String get yourLastSavedEventsRemainVisibleWith =>
      'Your last saved events remain visible, with their collection time. Revoking permission stops new reads; it does not erase the saved copy.';

  @override
  String get connections => 'Connections';

  @override
  String get manageTheServicesThatBringContextTo =>
      'Manage the services that bring context to your day.';

  @override
  String get availableServices => 'Available services';

  @override
  String get bringEventsFromYourMacIntoYour =>
      'Bring events from your Mac into your day.';

  @override
  String get loadingCalendar => 'Loading calendar';

  @override
  String get dismissDialog => 'Dismiss dialog';

  @override
  String get readOnly => 'Read-only';

  @override
  String get couldNotStartFloeCore => 'Could not start Floe Core';

  @override
  String taskSummary(int remaining, int total) {
    final intl.NumberFormat totalNumberFormat =
        intl.NumberFormat.decimalPattern(localeName);
    final String totalString = totalNumberFormat.format(total);

    String _temp0 = intl.Intl.pluralLogic(
      remaining,
      locale: localeName,
      other: '$remaining tasks remaining',
      one: '1 task remaining',
      zero: 'No tasks remaining',
    );
    return '$_temp0 · $totalString total';
  }

  @override
  String notesCount(int count) {
    final intl.NumberFormat countNumberFormat =
        intl.NumberFormat.decimalPattern(localeName);
    final String countString = countNumberFormat.format(count);

    return 'All notes · $countString';
  }

  @override
  String dueAt(String time) {
    return 'Due $time';
  }

  @override
  String overdueItem(String label, String subtitle) {
    return '$label · $subtitle · Overdue';
  }

  @override
  String deleteItemLabel(String title) {
    return 'Delete $title';
  }

  @override
  String deleteItemMessage(String title) {
    return '“$title” will be removed from your day.';
  }

  @override
  String capturedText(String text) {
    return 'Captured “$text”';
  }

  @override
  String lastCollectedCache(String time) {
    return 'Last collected $time · saved data';
  }

  @override
  String exclusiveDate(String date) {
    return '$date · exclusive';
  }

  @override
  String zoomTimes(int value) {
    String _temp0 = intl.Intl.pluralLogic(
      value,
      locale: localeName,
      other: '$value times',
      one: '1 time',
    );
    return '$_temp0';
  }

  @override
  String get actionNewProposal => 'Plan a Calendar event';

  @override
  String get actionTitle => 'Event title';

  @override
  String get actionPrepareReview => 'Prepare for review';

  @override
  String get actionProposalExplanation =>
      'Choose a connected calendar and future date and time, up to 24 hours apart. Floe will follow your action permission and ask for review when needed.';

  @override
  String get actionFormInvalid =>
      'Check this value and the future date and time.';

  @override
  String get actionWriteEnabled =>
      'Explicit approval creates one event after fresh checks. No guests or alerts. Ambiguous results are recovered by lookup only.';

  @override
  String get actionCheckingCreating =>
      'Checking permission, target and conflicts, then creating once…';

  @override
  String get actionLookingUp =>
      'Checking Calendar for this exact event. No new event is being created…';

  @override
  String get actionCollecting => 'Collecting the created event into Today…';

  @override
  String get actionCollected =>
      'The created event was collected into your saved calendar timeline.';

  @override
  String get actionReadFailed =>
      'The event was created, but could not be collected. Retry the read, not creation.';

  @override
  String get actionExecuteApproved => 'Create this approved event';

  @override
  String get actionCheckCalendar => 'Check Calendar for this event';

  @override
  String get actionRetryRead => 'Retry Calendar read';

  @override
  String get actionApproveCreate => 'Approve & create';

  @override
  String get calendarProposals => 'Review requests';

  @override
  String get actionPending => 'Awaiting your decision';

  @override
  String get actionApproved =>
      'Approval saved. No event has been created by this app.';

  @override
  String get actionRejected => 'Declined. No event was created.';

  @override
  String get actionExecuting =>
      'Execution was interrupted or is in progress. Do not create a replacement; inspect Calendar.';

  @override
  String get actionBlocked =>
      'Blocked. A fresh action request is required before any future execution.';

  @override
  String get actionUnknown =>
      'The result is unconfirmed. Inspect the original calendar; do not create a replacement.';

  @override
  String get actionSucceeded => 'Created in Calendar.';

  @override
  String get actionReloadRequired =>
      'The saved state could not be confirmed. Reload reviews before another decision.';

  @override
  String get actionLoading => 'Reading or saving review state…';

  @override
  String get actionEmpty => 'Nothing needs your review.';

  @override
  String get actionReview => 'Review request';

  @override
  String get actionReload => 'Reload reviews';

  @override
  String get actionMissing => 'This review request is no longer available.';

  @override
  String get actionDestination => 'Destination calendar';

  @override
  String get actionStart => 'Starts';

  @override
  String get actionEnd => 'Ends';

  @override
  String get actionPerson => 'Person';

  @override
  String get actionExpires => 'Approval expires';

  @override
  String get actionProposalId => 'Action request ID';

  @override
  String get actionExecutionId => 'Execution ID';

  @override
  String get actionApprovedAt => 'Approved at';

  @override
  String get actionExternalId => 'External event ID';

  @override
  String get actionReason => 'Recorded reason';

  @override
  String get actionNoExtras =>
      'Only this event. No guests, alerts or repeat schedule. Existing events stay unchanged.';

  @override
  String get actionTechnicalDetails => 'Technical details';

  @override
  String get actionWhen => 'When';

  @override
  String get actionProvider => 'Provider';

  @override
  String get actionCalendarId => 'Calendar ID';

  @override
  String get actionConflictReason =>
      'Another event overlaps this time. Nothing was created.';

  @override
  String get actionExpiredReason =>
      'This review request has expired. Nothing was created.';

  @override
  String get actionPermissionReason =>
      'Calendar write access is unavailable. Nothing was created.';

  @override
  String get actionChangedReason =>
      'The calendar connection has changed. Nothing was created.';

  @override
  String get actionTimezoneReason =>
      'This event’s time could not be verified. Nothing was created.';

  @override
  String get actionUnavailableReason =>
      'Floe could not safely create this event. Nothing was created.';

  @override
  String get actionWriteDisabled =>
      'Calendar writing is not enabled. Approval only saves your decision; it does not schedule or queue an event.';

  @override
  String get actionApprovalUnavailable =>
      'This action can’t be approved right now. Refresh its status or reconnect Calendar.';

  @override
  String get actionDecline => 'Decline';

  @override
  String get actionSaveApproval => 'Save approval only';

  @override
  String get agentCreateStorage => 'Set up secure storage';

  @override
  String get agentUnlockStorage => 'Unlock conversation storage';

  @override
  String get agentLockStorage => 'Lock conversation storage';

  @override
  String get agentStorageMissing =>
      'Set up encrypted storage on this device before trying a sample conversation. Your device key stays in its secure key store.';

  @override
  String get agentStorageLocked =>
      'Conversation storage is locked. Unlock it to resume your saved sample conversation.';

  @override
  String get agentStorageUnavailable =>
      'Floe couldn’t access secure storage. Check your device access and try again. Your saved conversations haven’t been replaced.';

  @override
  String get agentSecureSampleBoundary =>
      'Sample questions only, saved in encrypted storage. Personal chat, real models and connected sources aren’t enabled yet.';

  @override
  String connectedServicesCount(int count) {
    final intl.NumberFormat countNumberFormat =
        intl.NumberFormat.decimalPattern(localeName);
    final String countString = countNumberFormat.format(count);

    return 'Connected services · $countString';
  }
}
