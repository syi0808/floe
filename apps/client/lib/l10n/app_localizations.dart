import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations)!;
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[Locale('en')];

  /// No description provided for @settings.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get settings;

  /// No description provided for @calendarCouldNotBeCollectedCheckAccess.
  ///
  /// In en, this message translates to:
  /// **'Calendar could not be collected. Check access and try again.'**
  String get calendarCouldNotBeCollectedCheckAccess;

  /// No description provided for @showingSavedEventsCalendarChangesCouldNot.
  ///
  /// In en, this message translates to:
  /// **'Showing saved events. Calendar changes could not be collected.'**
  String get showingSavedEventsCalendarChangesCouldNot;

  /// No description provided for @manageConnection.
  ///
  /// In en, this message translates to:
  /// **'Manage connection'**
  String get manageConnection;

  /// No description provided for @taskCompleted.
  ///
  /// In en, this message translates to:
  /// **'Task completed.'**
  String get taskCompleted;

  /// No description provided for @taskMarkedIncomplete.
  ///
  /// In en, this message translates to:
  /// **'Task marked incomplete.'**
  String get taskMarkedIncomplete;

  /// No description provided for @undo.
  ///
  /// In en, this message translates to:
  /// **'Undo'**
  String get undo;

  /// No description provided for @today.
  ///
  /// In en, this message translates to:
  /// **'Today'**
  String get today;

  /// No description provided for @tasks.
  ///
  /// In en, this message translates to:
  /// **'Tasks'**
  String get tasks;

  /// No description provided for @notes.
  ///
  /// In en, this message translates to:
  /// **'Notes'**
  String get notes;

  /// No description provided for @connect.
  ///
  /// In en, this message translates to:
  /// **'Connect'**
  String get connect;

  /// No description provided for @previousDay.
  ///
  /// In en, this message translates to:
  /// **'Previous day'**
  String get previousDay;

  /// No description provided for @nextDay.
  ///
  /// In en, this message translates to:
  /// **'Next day'**
  String get nextDay;

  /// No description provided for @refreshCalendar.
  ///
  /// In en, this message translates to:
  /// **'Refresh calendar'**
  String get refreshCalendar;

  /// No description provided for @task.
  ///
  /// In en, this message translates to:
  /// **'Task'**
  String get task;

  /// No description provided for @newTask.
  ///
  /// In en, this message translates to:
  /// **'New task'**
  String get newTask;

  /// No description provided for @noTasksYet.
  ///
  /// In en, this message translates to:
  /// **'No tasks yet.'**
  String get noTasksYet;

  /// No description provided for @personal.
  ///
  /// In en, this message translates to:
  /// **'Personal'**
  String get personal;

  /// No description provided for @searchNotes.
  ///
  /// In en, this message translates to:
  /// **'Search notes'**
  String get searchNotes;

  /// No description provided for @filter.
  ///
  /// In en, this message translates to:
  /// **'Filter'**
  String get filter;

  /// No description provided for @newNote.
  ///
  /// In en, this message translates to:
  /// **'New note'**
  String get newNote;

  /// No description provided for @noNotesFound.
  ///
  /// In en, this message translates to:
  /// **'No notes found'**
  String get noNotesFound;

  /// No description provided for @tryADifferentSearchOrClearThe.
  ///
  /// In en, this message translates to:
  /// **'Try a different search or clear the current filter.'**
  String get tryADifferentSearchOrClearThe;

  /// No description provided for @clearFilters.
  ///
  /// In en, this message translates to:
  /// **'Clear filters'**
  String get clearFilters;

  /// No description provided for @writeAThoughtDecisionOrDetailTo.
  ///
  /// In en, this message translates to:
  /// **'Write a thought, decision, or detail to remember.'**
  String get writeAThoughtDecisionOrDetailTo;

  /// No description provided for @couldNotSavePleaseTryAgain.
  ///
  /// In en, this message translates to:
  /// **'Could not save. Please try again.'**
  String get couldNotSavePleaseTryAgain;

  /// No description provided for @cancel.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get cancel;

  /// No description provided for @saving.
  ///
  /// In en, this message translates to:
  /// **'Saving…'**
  String get saving;

  /// No description provided for @saveNote.
  ///
  /// In en, this message translates to:
  /// **'Save note'**
  String get saveNote;

  /// No description provided for @noDescriptionYet.
  ///
  /// In en, this message translates to:
  /// **'No description yet.'**
  String get noDescriptionYet;

  /// No description provided for @due.
  ///
  /// In en, this message translates to:
  /// **'Due'**
  String get due;

  /// No description provided for @noDueDate.
  ///
  /// In en, this message translates to:
  /// **'No due date'**
  String get noDueDate;

  /// No description provided for @timeContext.
  ///
  /// In en, this message translates to:
  /// **'Time context'**
  String get timeContext;

  /// No description provided for @notScheduled.
  ///
  /// In en, this message translates to:
  /// **'Not scheduled'**
  String get notScheduled;

  /// No description provided for @calendar.
  ///
  /// In en, this message translates to:
  /// **'Calendar'**
  String get calendar;

  /// No description provided for @subtasks.
  ///
  /// In en, this message translates to:
  /// **'Subtasks'**
  String get subtasks;

  /// No description provided for @noSubtasksYet.
  ///
  /// In en, this message translates to:
  /// **'No subtasks yet.'**
  String get noSubtasksYet;

  /// No description provided for @addASubtask.
  ///
  /// In en, this message translates to:
  /// **'Add a subtask'**
  String get addASubtask;

  /// No description provided for @floeSuggests.
  ///
  /// In en, this message translates to:
  /// **'Floe suggests'**
  String get floeSuggests;

  /// No description provided for @dismissSuggestion.
  ///
  /// In en, this message translates to:
  /// **'Dismiss suggestion'**
  String get dismissSuggestion;

  /// No description provided for @reviewTheContextBeforeStartingThisTask.
  ///
  /// In en, this message translates to:
  /// **'Review the context before starting this task?'**
  String get reviewTheContextBeforeStartingThisTask;

  /// No description provided for @reviewNow.
  ///
  /// In en, this message translates to:
  /// **'Review now'**
  String get reviewNow;

  /// No description provided for @snooze.
  ///
  /// In en, this message translates to:
  /// **'Snooze'**
  String get snooze;

  /// No description provided for @noLinkedNotes.
  ///
  /// In en, this message translates to:
  /// **'No linked notes.'**
  String get noLinkedNotes;

  /// No description provided for @updatedThisMorning.
  ///
  /// In en, this message translates to:
  /// **'Updated this morning'**
  String get updatedThisMorning;

  /// No description provided for @taskOptions.
  ///
  /// In en, this message translates to:
  /// **'Task options'**
  String get taskOptions;

  /// No description provided for @markIncomplete.
  ///
  /// In en, this message translates to:
  /// **'Mark incomplete'**
  String get markIncomplete;

  /// No description provided for @completeTask.
  ///
  /// In en, this message translates to:
  /// **'Complete task'**
  String get completeTask;

  /// No description provided for @personalNote.
  ///
  /// In en, this message translates to:
  /// **'Personal note'**
  String get personalNote;

  /// No description provided for @close.
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get close;

  /// No description provided for @thisIsYourOriginalNoteEditingIs.
  ///
  /// In en, this message translates to:
  /// **'This is your original note. Editing is not available yet.'**
  String get thisIsYourOriginalNoteEditingIs;

  /// No description provided for @reviewWithFloe.
  ///
  /// In en, this message translates to:
  /// **'Review with Floe'**
  String get reviewWithFloe;

  /// No description provided for @timeNotSet.
  ///
  /// In en, this message translates to:
  /// **'Time not set'**
  String get timeNotSet;

  /// No description provided for @todaySThought.
  ///
  /// In en, this message translates to:
  /// **'Today\'s thought'**
  String get todaySThought;

  /// No description provided for @event.
  ///
  /// In en, this message translates to:
  /// **'Event'**
  String get event;

  /// No description provided for @note.
  ///
  /// In en, this message translates to:
  /// **'Note'**
  String get note;

  /// No description provided for @readOnlyEventManagedInItsOriginal.
  ///
  /// In en, this message translates to:
  /// **'Read-only event managed in its original calendar'**
  String get readOnlyEventManagedInItsOriginal;

  /// No description provided for @deleteThisItem.
  ///
  /// In en, this message translates to:
  /// **'Delete this item?'**
  String get deleteThisItem;

  /// No description provided for @delete.
  ///
  /// In en, this message translates to:
  /// **'Delete'**
  String get delete;

  /// No description provided for @dismissCapture.
  ///
  /// In en, this message translates to:
  /// **'Dismiss capture'**
  String get dismissCapture;

  /// No description provided for @aThoughtForYourDay.
  ///
  /// In en, this message translates to:
  /// **'A thought for your day...'**
  String get aThoughtForYourDay;

  /// No description provided for @saveCapture.
  ///
  /// In en, this message translates to:
  /// **'Save capture'**
  String get saveCapture;

  /// No description provided for @couldNotLoadYourDay.
  ///
  /// In en, this message translates to:
  /// **'Could not load your day'**
  String get couldNotLoadYourDay;

  /// No description provided for @anUnknownErrorOccurred.
  ///
  /// In en, this message translates to:
  /// **'An unknown error occurred.'**
  String get anUnknownErrorOccurred;

  /// No description provided for @tryAgain.
  ///
  /// In en, this message translates to:
  /// **'Try again'**
  String get tryAgain;

  /// No description provided for @whereShouldThisGo.
  ///
  /// In en, this message translates to:
  /// **'Where should this go?'**
  String get whereShouldThisGo;

  /// No description provided for @originalInput.
  ///
  /// In en, this message translates to:
  /// **'Original input'**
  String get originalInput;

  /// No description provided for @thought.
  ///
  /// In en, this message translates to:
  /// **'Thought'**
  String get thought;

  /// No description provided for @title.
  ///
  /// In en, this message translates to:
  /// **'Title'**
  String get title;

  /// No description provided for @content.
  ///
  /// In en, this message translates to:
  /// **'Content'**
  String get content;

  /// No description provided for @later.
  ///
  /// In en, this message translates to:
  /// **'Later'**
  String get later;

  /// No description provided for @classifyAndAdd.
  ///
  /// In en, this message translates to:
  /// **'Classify and add'**
  String get classifyAndAdd;

  /// No description provided for @pleaseEnterSomeText.
  ///
  /// In en, this message translates to:
  /// **'Please enter some text.'**
  String get pleaseEnterSomeText;

  /// No description provided for @endTimeMustBeAfterStartTime.
  ///
  /// In en, this message translates to:
  /// **'End time must be after start time.'**
  String get endTimeMustBeAfterStartTime;

  /// No description provided for @start.
  ///
  /// In en, this message translates to:
  /// **'Start'**
  String get start;

  /// No description provided for @end.
  ///
  /// In en, this message translates to:
  /// **'End'**
  String get end;

  /// No description provided for @setADueDate.
  ///
  /// In en, this message translates to:
  /// **'Set a due date'**
  String get setADueDate;

  /// No description provided for @thisFeatureIsNotAvailableYet.
  ///
  /// In en, this message translates to:
  /// **'This feature is not available yet.'**
  String get thisFeatureIsNotAvailableYet;

  /// No description provided for @allDay.
  ///
  /// In en, this message translates to:
  /// **'All day'**
  String get allDay;

  /// No description provided for @zoomOut.
  ///
  /// In en, this message translates to:
  /// **'Zoom out'**
  String get zoomOut;

  /// No description provided for @zoomIn.
  ///
  /// In en, this message translates to:
  /// **'Zoom in'**
  String get zoomIn;

  /// No description provided for @aLittleBreathingRoom.
  ///
  /// In en, this message translates to:
  /// **'A little breathing room.'**
  String get aLittleBreathingRoom;

  /// No description provided for @yourDayIsStillEmpty.
  ///
  /// In en, this message translates to:
  /// **'Your day is still empty.'**
  String get yourDayIsStillEmpty;

  /// No description provided for @noSavedEventsForThisDay.
  ///
  /// In en, this message translates to:
  /// **'No saved events for this day.'**
  String get noSavedEventsForThisDay;

  /// No description provided for @viewConnectedCalendars.
  ///
  /// In en, this message translates to:
  /// **'View connected calendars'**
  String get viewConnectedCalendars;

  /// No description provided for @savedInFloe.
  ///
  /// In en, this message translates to:
  /// **'Saved in Floe'**
  String get savedInFloe;

  /// No description provided for @sourceTimeZone.
  ///
  /// In en, this message translates to:
  /// **'Source time zone'**
  String get sourceTimeZone;

  /// No description provided for @localTime.
  ///
  /// In en, this message translates to:
  /// **'Local time'**
  String get localTime;

  /// No description provided for @lastCollected.
  ///
  /// In en, this message translates to:
  /// **'Last collected'**
  String get lastCollected;

  /// No description provided for @allDayBoundary.
  ///
  /// In en, this message translates to:
  /// **'All-day boundary'**
  String get allDayBoundary;

  /// No description provided for @manageThisEventInItsOriginalCalendar.
  ///
  /// In en, this message translates to:
  /// **'Manage this event in its original calendar. Floe has no edit or delete action for imported events.'**
  String get manageThisEventInItsOriginalCalendar;

  /// No description provided for @sourceDetails.
  ///
  /// In en, this message translates to:
  /// **'Source details'**
  String get sourceDetails;

  /// No description provided for @connectionPerson.
  ///
  /// In en, this message translates to:
  /// **'Connection / Person'**
  String get connectionPerson;

  /// No description provided for @externalOccurrenceId.
  ///
  /// In en, this message translates to:
  /// **'External occurrence ID'**
  String get externalOccurrenceId;

  /// No description provided for @revision.
  ///
  /// In en, this message translates to:
  /// **'Revision'**
  String get revision;

  /// No description provided for @integration.
  ///
  /// In en, this message translates to:
  /// **'Integration'**
  String get integration;

  /// No description provided for @backToMyDay.
  ///
  /// In en, this message translates to:
  /// **'Back to my day'**
  String get backToMyDay;

  /// No description provided for @couldNotConnectOrCollectEventsCheck.
  ///
  /// In en, this message translates to:
  /// **'Could not connect or collect events. Check calendar access and try again.'**
  String get couldNotConnectOrCollectEventsCheck;

  /// No description provided for @connectCalendar.
  ///
  /// In en, this message translates to:
  /// **'Connect Calendar'**
  String get connectCalendar;

  /// No description provided for @eventsFromTheSelectedCalendarAreSaved.
  ///
  /// In en, this message translates to:
  /// **'Events from the selected calendar are saved on this device. macOS requires Full Access even for reading, but Floe never changes external events. Selecting a different calendar replaces the previous local copy.'**
  String get eventsFromTheSelectedCalendarAreSaved;

  /// No description provided for @continueAction.
  ///
  /// In en, this message translates to:
  /// **'Continue'**
  String get continueAction;

  /// No description provided for @noCalendarsAreAvailableAddACalendar.
  ///
  /// In en, this message translates to:
  /// **'No calendars are available. Add a calendar in macOS Calendar first.'**
  String get noCalendarsAreAvailableAddACalendar;

  /// No description provided for @chooseACalendar.
  ///
  /// In en, this message translates to:
  /// **'Choose a calendar'**
  String get chooseACalendar;

  /// No description provided for @calendarAccessWasDeniedOrRevokedAllow.
  ///
  /// In en, this message translates to:
  /// **'Calendar access was denied or revoked. Allow access in Settings, then try again.'**
  String get calendarAccessWasDeniedOrRevokedAllow;

  /// No description provided for @theSelectedCalendarIsUnavailablePleaseReconnect.
  ///
  /// In en, this message translates to:
  /// **'The selected calendar is unavailable. Please reconnect.'**
  String get theSelectedCalendarIsUnavailablePleaseReconnect;

  /// No description provided for @couldNotCollectEventsShowingTheLast.
  ///
  /// In en, this message translates to:
  /// **'Could not collect events. Showing the last saved data.'**
  String get couldNotCollectEventsShowingTheLast;

  /// No description provided for @notCollectedYet.
  ///
  /// In en, this message translates to:
  /// **'Not collected yet'**
  String get notCollectedYet;

  /// No description provided for @macosCalendar.
  ///
  /// In en, this message translates to:
  /// **'macOS Calendar'**
  String get macosCalendar;

  /// No description provided for @calendarsAlreadyOnThisMac.
  ///
  /// In en, this message translates to:
  /// **'Calendars already on this Mac'**
  String get calendarsAlreadyOnThisMac;

  /// No description provided for @bringYourCalendarIntoOneDayFloe.
  ///
  /// In en, this message translates to:
  /// **'Bring your calendar into one day. Floe reads events; it never creates, edits, or deletes anything in Calendar.'**
  String get bringYourCalendarIntoOneDayFloe;

  /// No description provided for @connectedCalendar.
  ///
  /// In en, this message translates to:
  /// **'Connected calendar'**
  String get connectedCalendar;

  /// No description provided for @makeRoomForYourDay.
  ///
  /// In en, this message translates to:
  /// **'Make room for your day.'**
  String get makeRoomForYourDay;

  /// No description provided for @chooseACalendarToStartThisClient.
  ///
  /// In en, this message translates to:
  /// **'Choose a calendar to start. This client currently supports one calendar at a time.'**
  String get chooseACalendarToStartThisClient;

  /// No description provided for @person.
  ///
  /// In en, this message translates to:
  /// **'Person'**
  String get person;

  /// No description provided for @youThisDevice.
  ///
  /// In en, this message translates to:
  /// **'You · this device'**
  String get youThisDevice;

  /// No description provided for @storedRangeLabel.
  ///
  /// In en, this message translates to:
  /// **'Stored range'**
  String get storedRangeLabel;

  /// No description provided for @storedRange.
  ///
  /// In en, this message translates to:
  /// **'{start} – {end} (exclusive)'**
  String storedRange(String start, String end);

  /// No description provided for @lastSuccessfulRead.
  ///
  /// In en, this message translates to:
  /// **'Last successful read'**
  String get lastSuccessfulRead;

  /// No description provided for @refreshSelectedDay.
  ///
  /// In en, this message translates to:
  /// **'Refresh selected day'**
  String get refreshSelectedDay;

  /// No description provided for @reconnectOrChange.
  ///
  /// In en, this message translates to:
  /// **'Reconnect or change'**
  String get reconnectOrChange;

  /// No description provided for @manageAccess.
  ///
  /// In en, this message translates to:
  /// **'Manage access'**
  String get manageAccess;

  /// No description provided for @yourOwnRhythm.
  ///
  /// In en, this message translates to:
  /// **'Your own rhythm'**
  String get yourOwnRhythm;

  /// No description provided for @aFewThingsThatBelongToYou.
  ///
  /// In en, this message translates to:
  /// **'A few things that belong to you, not your calendar.'**
  String get aFewThingsThatBelongToYou;

  /// No description provided for @noTasksForToday.
  ///
  /// In en, this message translates to:
  /// **'No tasks for today.'**
  String get noTasksForToday;

  /// No description provided for @seeYourTasks.
  ///
  /// In en, this message translates to:
  /// **'See your tasks'**
  String get seeYourTasks;

  /// No description provided for @aNoteToSelf.
  ///
  /// In en, this message translates to:
  /// **'A note to self'**
  String get aNoteToSelf;

  /// No description provided for @leaveALittleRoomBetweenThingsNot.
  ///
  /// In en, this message translates to:
  /// **'Leave a little room between things. Not every empty space needs filling.'**
  String get leaveALittleRoomBetweenThingsNot;

  /// No description provided for @savedInFloeStaysWhenYouDisconnect.
  ///
  /// In en, this message translates to:
  /// **'Saved in Floe · stays when you disconnect'**
  String get savedInFloeStaysWhenYouDisconnect;

  /// No description provided for @wonderingWhereAnEventCameFromOpen.
  ///
  /// In en, this message translates to:
  /// **'Wondering where an event came from?\nOpen it to see its source and time zone.'**
  String get wonderingWhereAnEventCameFromOpen;

  /// No description provided for @backToConnections.
  ///
  /// In en, this message translates to:
  /// **'Back to connections'**
  String get backToConnections;

  /// No description provided for @calendarIntegrationIsUnavailableInThisPreview.
  ///
  /// In en, this message translates to:
  /// **'Calendar integration is unavailable in this preview. Use the native macOS client to connect.'**
  String get calendarIntegrationIsUnavailableInThisPreview;

  /// No description provided for @aClearBoundary.
  ///
  /// In en, this message translates to:
  /// **'A clear boundary.'**
  String get aClearBoundary;

  /// No description provided for @eventsAreSavedOnThisMacFloe.
  ///
  /// In en, this message translates to:
  /// **'Events are saved on this Mac. Floe never creates, edits, or deletes events in your connected calendar.'**
  String get eventsAreSavedOnThisMacFloe;

  /// No description provided for @macosCallsThisFullAccessEvenFor.
  ///
  /// In en, this message translates to:
  /// **'macOS calls this “Full Access,” even for reading. That OS permission does not enable writes in Floe.'**
  String get macosCallsThisFullAccessEvenFor;

  /// No description provided for @whatHappensOffline.
  ///
  /// In en, this message translates to:
  /// **'What happens offline?'**
  String get whatHappensOffline;

  /// No description provided for @yourLastSavedEventsRemainVisibleWith.
  ///
  /// In en, this message translates to:
  /// **'Your last saved events remain visible, with their collection time. Revoking permission stops new reads; it does not erase the saved copy.'**
  String get yourLastSavedEventsRemainVisibleWith;

  /// No description provided for @connections.
  ///
  /// In en, this message translates to:
  /// **'Connections'**
  String get connections;

  /// No description provided for @manageTheServicesThatBringContextTo.
  ///
  /// In en, this message translates to:
  /// **'Manage the services that bring context to your day.'**
  String get manageTheServicesThatBringContextTo;

  /// No description provided for @availableServices.
  ///
  /// In en, this message translates to:
  /// **'Available services'**
  String get availableServices;

  /// No description provided for @bringEventsFromYourMacIntoYour.
  ///
  /// In en, this message translates to:
  /// **'Bring events from your Mac into your day.'**
  String get bringEventsFromYourMacIntoYour;

  /// No description provided for @loadingCalendar.
  ///
  /// In en, this message translates to:
  /// **'Loading calendar'**
  String get loadingCalendar;

  /// No description provided for @dismissDialog.
  ///
  /// In en, this message translates to:
  /// **'Dismiss dialog'**
  String get dismissDialog;

  /// No description provided for @readOnly.
  ///
  /// In en, this message translates to:
  /// **'Read-only'**
  String get readOnly;

  /// No description provided for @couldNotStartFloeCore.
  ///
  /// In en, this message translates to:
  /// **'Could not start Floe Core'**
  String get couldNotStartFloeCore;

  /// No description provided for @taskSummary.
  ///
  /// In en, this message translates to:
  /// **'{remaining, plural, =0{No tasks remaining} one{1 task remaining} other{{remaining} tasks remaining}} · {total} total'**
  String taskSummary(int remaining, int total);

  /// No description provided for @notesCount.
  ///
  /// In en, this message translates to:
  /// **'All notes · {count}'**
  String notesCount(int count);

  /// No description provided for @dueAt.
  ///
  /// In en, this message translates to:
  /// **'Due {time}'**
  String dueAt(String time);

  /// No description provided for @overdueItem.
  ///
  /// In en, this message translates to:
  /// **'{label} · {subtitle} · Overdue'**
  String overdueItem(String label, String subtitle);

  /// No description provided for @deleteItemLabel.
  ///
  /// In en, this message translates to:
  /// **'Delete {title}'**
  String deleteItemLabel(String title);

  /// No description provided for @deleteItemMessage.
  ///
  /// In en, this message translates to:
  /// **'“{title}” will be removed from your day.'**
  String deleteItemMessage(String title);

  /// No description provided for @capturedText.
  ///
  /// In en, this message translates to:
  /// **'Captured “{text}”'**
  String capturedText(String text);

  /// No description provided for @lastCollectedCache.
  ///
  /// In en, this message translates to:
  /// **'Last collected {time} · saved data'**
  String lastCollectedCache(String time);

  /// No description provided for @exclusiveDate.
  ///
  /// In en, this message translates to:
  /// **'{date} · exclusive'**
  String exclusiveDate(String date);

  /// No description provided for @dateAndZone.
  ///
  /// In en, this message translates to:
  /// **'{date} · {zone}'**
  String dateAndZone(String date, String zone);

  /// No description provided for @zoomTimes.
  ///
  /// In en, this message translates to:
  /// **'{value, plural, one{1 time} other{{value} times}}'**
  String zoomTimes(int value);

  /// No description provided for @connectedServicesCount.
  ///
  /// In en, this message translates to:
  /// **'Connected services · {count}'**
  String connectedServicesCount(int count);
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppLocalizationsEn();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
