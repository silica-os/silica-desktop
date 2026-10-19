import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';
import 'app_localizations_it.dart';

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

  static AppLocalizations? of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations);
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
  static const List<Locale> supportedLocales = <Locale>[
    Locale('en'),
    Locale('it'),
  ];

  /// Control Center module subtitle when the service is missing
  ///
  /// In en, this message translates to:
  /// **'Unavailable'**
  String get ccUnavailable;

  /// Control Center module subtitle: switched off
  ///
  /// In en, this message translates to:
  /// **'Off'**
  String get ccOff;

  /// Bluetooth module subtitle: powered
  ///
  /// In en, this message translates to:
  /// **'On'**
  String get ccOn;

  /// Wi-Fi module subtitle when only a cable is connected
  ///
  /// In en, this message translates to:
  /// **'Wired network'**
  String get ccWired;

  /// Wi-Fi module subtitle: on but not connected
  ///
  /// In en, this message translates to:
  /// **'Not connected'**
  String get ccNotConnected;

  /// Control Center module title
  ///
  /// In en, this message translates to:
  /// **'Do not disturb'**
  String get ccDoNotDisturb;

  /// Do not disturb module subtitle when active
  ///
  /// In en, this message translates to:
  /// **'On'**
  String get ccDoNotDisturbOn;

  /// Control Center module title
  ///
  /// In en, this message translates to:
  /// **'Battery'**
  String get ccBattery;

  /// Battery module subtitle
  ///
  /// In en, this message translates to:
  /// **'{percent}%'**
  String ccBatteryLevel(int percent);

  /// Battery module subtitle while charging
  ///
  /// In en, this message translates to:
  /// **'{percent}% · charging'**
  String ccBatteryCharging(int percent);

  /// Display module and HUD title (brightness)
  ///
  /// In en, this message translates to:
  /// **'Display'**
  String get ccDisplay;

  /// Sound module title
  ///
  /// In en, this message translates to:
  /// **'Sound'**
  String get ccSound;

  /// Media module: track without a title
  ///
  /// In en, this message translates to:
  /// **'Untitled'**
  String get mediaUntitled;

  /// Media module when no player is active (two lines)
  ///
  /// In en, this message translates to:
  /// **'Nothing\nplaying'**
  String get mediaNothingPlaying;

  /// Wi-Fi detail section header
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =1{Known network} other{Known networks}}'**
  String knownNetworks(int count);

  /// Wi-Fi detail: expandable section
  ///
  /// In en, this message translates to:
  /// **'Other networks'**
  String get otherNetworks;

  /// Sound detail section header
  ///
  /// In en, this message translates to:
  /// **'Output'**
  String get soundOutput;

  /// Volume HUD title when the output has no name
  ///
  /// In en, this message translates to:
  /// **'Audio output'**
  String get hudAudioOutput;

  /// Dock stack menu section
  ///
  /// In en, this message translates to:
  /// **'Sort by'**
  String get stackSortBy;

  /// Sort option
  ///
  /// In en, this message translates to:
  /// **'Name'**
  String get sortName;

  /// Sort option
  ///
  /// In en, this message translates to:
  /// **'Date added'**
  String get sortDateAdded;

  /// Sort option
  ///
  /// In en, this message translates to:
  /// **'Date modified'**
  String get sortDateModified;

  /// Sort option
  ///
  /// In en, this message translates to:
  /// **'Date created'**
  String get sortDateCreated;

  /// Sort option
  ///
  /// In en, this message translates to:
  /// **'Kind'**
  String get sortKind;

  /// Dock stack menu section
  ///
  /// In en, this message translates to:
  /// **'Show as'**
  String get stackShowAs;

  /// Show the Dock folder as its folder icon
  ///
  /// In en, this message translates to:
  /// **'Folder'**
  String get stackFolder;

  /// Show the Dock folder as a pile of its files
  ///
  /// In en, this message translates to:
  /// **'Stack'**
  String get stackStack;

  /// Dock stack menu section
  ///
  /// In en, this message translates to:
  /// **'View content as'**
  String get stackViewContentAs;

  /// Stack view option
  ///
  /// In en, this message translates to:
  /// **'Fan'**
  String get stackFan;

  /// Stack view option
  ///
  /// In en, this message translates to:
  /// **'Grid'**
  String get stackGrid;

  /// Stack view option
  ///
  /// In en, this message translates to:
  /// **'List'**
  String get stackList;

  /// Stack view option
  ///
  /// In en, this message translates to:
  /// **'Automatic'**
  String get stackAutomatic;

  /// Menu item or section with options
  ///
  /// In en, this message translates to:
  /// **'Options'**
  String get options;

  /// Dock folder menu item
  ///
  /// In en, this message translates to:
  /// **'Show in enclosing folder'**
  String get showInEnclosingFolder;

  /// Dock menu item opening a folder or window
  ///
  /// In en, this message translates to:
  /// **'Open \"{name}\"'**
  String openNamed(String name);

  /// Dock separator menu
  ///
  /// In en, this message translates to:
  /// **'Turn magnification off'**
  String get magnificationOff;

  /// Dock separator menu
  ///
  /// In en, this message translates to:
  /// **'Turn magnification on'**
  String get magnificationOn;

  /// Dock separator menu
  ///
  /// In en, this message translates to:
  /// **'Show recent apps'**
  String get showRecentApps;

  /// Menu item
  ///
  /// In en, this message translates to:
  /// **'Open'**
  String get open;

  /// The Trash in the Dock
  ///
  /// In en, this message translates to:
  /// **'Trash'**
  String get trash;

  /// The Dock item that opens the Launcher
  ///
  /// In en, this message translates to:
  /// **'Apps'**
  String get dockApps;

  /// File manager menu in the Dock
  ///
  /// In en, this message translates to:
  /// **'New Window'**
  String get newWindow;

  /// Trash menu item (asks for confirmation)
  ///
  /// In en, this message translates to:
  /// **'Empty Trash…'**
  String get emptyTrashEllipsis;

  /// App options in the Dock
  ///
  /// In en, this message translates to:
  /// **'Remove from Dock'**
  String get removeFromDock;

  /// App options in the Dock
  ///
  /// In en, this message translates to:
  /// **'Keep in Dock'**
  String get keepInDock;

  /// Dock app menu
  ///
  /// In en, this message translates to:
  /// **'Show all windows'**
  String get showAllWindows;

  /// Dock app menu
  ///
  /// In en, this message translates to:
  /// **'Quit'**
  String get quit;

  /// Capsule over an icon dragged out of the Dock
  ///
  /// In en, this message translates to:
  /// **'Remove'**
  String get dockRemove;

  /// Label over the Trash while an app or folder icon of the Dock is dragged onto it
  ///
  /// In en, this message translates to:
  /// **'Remove from Dock'**
  String get dockRemoveFromDock;

  /// Last item of a fan stack when files are left out
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =1{1 more in the folder} other{{count} more in the folder}}'**
  String stackMoreInFolder(int count);

  /// Last item of a stack
  ///
  /// In en, this message translates to:
  /// **'Open folder'**
  String get openFolder;

  /// Empty Trash alert
  ///
  /// In en, this message translates to:
  /// **'Erase the items in the Trash for good?'**
  String get trashAlertTitle;

  /// Empty Trash alert
  ///
  /// In en, this message translates to:
  /// **'This cannot be undone.'**
  String get trashAlertMessage;

  /// Empty Trash alert button
  ///
  /// In en, this message translates to:
  /// **'Empty Trash'**
  String get emptyTrash;

  /// Button
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get cancel;

  /// Faro search field placeholder
  ///
  /// In en, this message translates to:
  /// **'Search with Faro'**
  String get faroPlaceholder;

  /// Faro result subtitle for apps
  ///
  /// In en, this message translates to:
  /// **'Application'**
  String get faroApplication;

  /// Inline completion in the launcher field
  ///
  /// In en, this message translates to:
  /// **'{rest} — Open'**
  String completionOpen(String rest);

  /// Launcher search field placeholder
  ///
  /// In en, this message translates to:
  /// **'Applications'**
  String get launcherPlaceholder;

  /// Launcher category
  ///
  /// In en, this message translates to:
  /// **'Productivity and finance'**
  String get catProductivity;

  /// Launcher category
  ///
  /// In en, this message translates to:
  /// **'Utilities'**
  String get catUtilities;

  /// Launcher category
  ///
  /// In en, this message translates to:
  /// **'Creativity'**
  String get catCreativity;

  /// Launcher category
  ///
  /// In en, this message translates to:
  /// **'Entertainment'**
  String get catEntertainment;

  /// Launcher category
  ///
  /// In en, this message translates to:
  /// **'Social'**
  String get catSocial;

  /// Launcher category
  ///
  /// In en, this message translates to:
  /// **'Information and reading'**
  String get catReading;

  /// Launcher category for the rest
  ///
  /// In en, this message translates to:
  /// **'Other'**
  String get catOther;

  /// Force quit panel title
  ///
  /// In en, this message translates to:
  /// **'Quit apps'**
  String get forceQuitTitle;

  /// Force quit panel explanation
  ///
  /// In en, this message translates to:
  /// **'If an app does not respond, choose it in the list and click \"Quit\".'**
  String get forceQuitHelp;

  /// Force quit panel hint
  ///
  /// In en, this message translates to:
  /// **'This panel also opens with ⌘⌥Esc.'**
  String get forceQuitShortcut;

  /// Force quit panel button
  ///
  /// In en, this message translates to:
  /// **'Quit'**
  String get forceQuitButton;

  /// Menu bar menu
  ///
  /// In en, this message translates to:
  /// **'File'**
  String get menuFile;

  /// Menu bar menu
  ///
  /// In en, this message translates to:
  /// **'Edit'**
  String get menuEdit;

  /// Menu bar menu
  ///
  /// In en, this message translates to:
  /// **'View'**
  String get menuView;

  /// Menu bar menu
  ///
  /// In en, this message translates to:
  /// **'Window'**
  String get menuWindow;

  /// Menu bar menu
  ///
  /// In en, this message translates to:
  /// **'Help'**
  String get menuHelp;

  /// Logo menu
  ///
  /// In en, this message translates to:
  /// **'About Silica'**
  String get aboutSilica;

  /// Logo menu
  ///
  /// In en, this message translates to:
  /// **'Settings…'**
  String get systemSettings;

  /// Logo menu: overview of windows and spaces
  ///
  /// In en, this message translates to:
  /// **'Panorama'**
  String get panorama;

  /// Logo menu
  ///
  /// In en, this message translates to:
  /// **'Show desktop'**
  String get showDesktop;

  /// Logo menu
  ///
  /// In en, this message translates to:
  /// **'Log out…'**
  String get logOut;

  /// App menu
  ///
  /// In en, this message translates to:
  /// **'About {app}'**
  String aboutApp(String app);

  /// App menu
  ///
  /// In en, this message translates to:
  /// **'Quit {app}'**
  String quitApp(String app);

  /// File menu
  ///
  /// In en, this message translates to:
  /// **'Close window'**
  String get closeWindow;

  /// File menu
  ///
  /// In en, this message translates to:
  /// **'Close All'**
  String get closeAll;

  /// View and Window menus, window popover
  ///
  /// In en, this message translates to:
  /// **'Full screen'**
  String get fullScreen;

  /// Window menu
  ///
  /// In en, this message translates to:
  /// **'Minimize'**
  String get minimize;

  /// Window menu
  ///
  /// In en, this message translates to:
  /// **'App windows'**
  String get appWindows;

  /// Window menu
  ///
  /// In en, this message translates to:
  /// **'Next space'**
  String get nextSpace;

  /// Window menu
  ///
  /// In en, this message translates to:
  /// **'Previous space'**
  String get previousSpace;

  /// App menu: the Services submenu
  ///
  /// In en, this message translates to:
  /// **'Services'**
  String get services;

  /// App menu › Services, disabled
  ///
  /// In en, this message translates to:
  /// **'No Services Available'**
  String get noServices;

  /// App menu
  ///
  /// In en, this message translates to:
  /// **'Hide {app}'**
  String hideApp(String app);

  /// App menu
  ///
  /// In en, this message translates to:
  /// **'Hide Others'**
  String get hideOthers;

  /// App menu
  ///
  /// In en, this message translates to:
  /// **'Show All'**
  String get showAll;

  /// Window menu: the window fills the screen
  ///
  /// In en, this message translates to:
  /// **'Fill'**
  String get fill;

  /// Window menu submenu
  ///
  /// In en, this message translates to:
  /// **'Move & Resize'**
  String get moveAndResize;

  /// Window menu › Move & Resize
  ///
  /// In en, this message translates to:
  /// **'Left'**
  String get tileLeft;

  /// Window menu › Move & Resize
  ///
  /// In en, this message translates to:
  /// **'Right'**
  String get tileRight;

  /// Window menu › Move & Resize
  ///
  /// In en, this message translates to:
  /// **'Top'**
  String get tileTop;

  /// Window menu › Move & Resize
  ///
  /// In en, this message translates to:
  /// **'Bottom'**
  String get tileBottom;

  /// Window menu › Move & Resize: two windows side by side
  ///
  /// In en, this message translates to:
  /// **'Left & Right'**
  String get arrangeLeftRight;

  /// Window menu › Move & Resize: one window on the left, two on the right
  ///
  /// In en, this message translates to:
  /// **'Left & Quarters'**
  String get arrangeLeftQuarters;

  /// Window menu › Move & Resize: four windows
  ///
  /// In en, this message translates to:
  /// **'Quarters'**
  String get arrangeQuarters;

  /// Window menu
  ///
  /// In en, this message translates to:
  /// **'Bring All to Front'**
  String get bringAllToFront;

  /// Help menu: the app's help
  ///
  /// In en, this message translates to:
  /// **'{app} Help'**
  String appHelp(String app);

  /// App menu: the app's settings
  ///
  /// In en, this message translates to:
  /// **'Settings…'**
  String get appSettings;

  /// Edit menu
  ///
  /// In en, this message translates to:
  /// **'Undo'**
  String get editUndo;

  /// Edit menu
  ///
  /// In en, this message translates to:
  /// **'Redo'**
  String get editRedo;

  /// Edit menu
  ///
  /// In en, this message translates to:
  /// **'Cut'**
  String get editCut;

  /// Edit menu
  ///
  /// In en, this message translates to:
  /// **'Copy'**
  String get editCopy;

  /// Edit menu
  ///
  /// In en, this message translates to:
  /// **'Paste'**
  String get editPaste;

  /// Edit menu
  ///
  /// In en, this message translates to:
  /// **'Delete'**
  String get editDelete;

  /// Edit menu
  ///
  /// In en, this message translates to:
  /// **'Select All'**
  String get editSelectAll;

  /// Window popover section
  ///
  /// In en, this message translates to:
  /// **'Half of the screen'**
  String get halfScreen;

  /// Window popover section
  ///
  /// In en, this message translates to:
  /// **'Arrange'**
  String get arrange;

  /// Notification Center header
  ///
  /// In en, this message translates to:
  /// **'Notifications'**
  String get notificationsTitle;

  /// Notification Center empty state
  ///
  /// In en, this message translates to:
  /// **'No Recent Notifications'**
  String get noNotifications;

  /// Notification group pill
  ///
  /// In en, this message translates to:
  /// **'Collapse'**
  String get collapse;

  /// Notification group pill
  ///
  /// In en, this message translates to:
  /// **'Clear'**
  String get clear;

  /// Notification banner button
  ///
  /// In en, this message translates to:
  /// **'Details'**
  String get details;

  /// Screenshot file name without extension; date YYYY-MM-DD, time HH.MM.SS
  ///
  /// In en, this message translates to:
  /// **'Screenshot {date} at {time}'**
  String screenshotFileName(String date, String time);

  /// Screenshot options section
  ///
  /// In en, this message translates to:
  /// **'Save to'**
  String get saveTo;

  /// Screenshot options section
  ///
  /// In en, this message translates to:
  /// **'Timer'**
  String get delay;

  /// Screenshot delay option
  ///
  /// In en, this message translates to:
  /// **'None'**
  String get delayNone;

  /// Screenshot delay option
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =1{1 second} other{{count} seconds}}'**
  String delaySeconds(int count);

  /// Screenshot option
  ///
  /// In en, this message translates to:
  /// **'Show Floating Thumbnail'**
  String get thumbnailAfterCapture;

  /// Screenshot bar button
  ///
  /// In en, this message translates to:
  /// **'Options ⌄'**
  String get optionsButton;

  /// Screenshot bar button
  ///
  /// In en, this message translates to:
  /// **'Capture'**
  String get capture;

  /// Tooltip of a button of the screenshot bar
  ///
  /// In en, this message translates to:
  /// **'Capture Entire Screen'**
  String get shotTipCaptureScreen;

  /// Tooltip of a button of the screenshot bar
  ///
  /// In en, this message translates to:
  /// **'Capture Selected Window'**
  String get shotTipCaptureWindow;

  /// Tooltip of a button of the screenshot bar
  ///
  /// In en, this message translates to:
  /// **'Capture Selected Portion'**
  String get shotTipCaptureSelection;

  /// Tooltip of a button of the screenshot bar
  ///
  /// In en, this message translates to:
  /// **'Record Entire Screen'**
  String get shotTipRecordScreen;

  /// Tooltip of a button of the screenshot bar
  ///
  /// In en, this message translates to:
  /// **'Record Selected Window'**
  String get shotTipRecordWindow;

  /// Tooltip of a button of the screenshot bar
  ///
  /// In en, this message translates to:
  /// **'Record Selected Portion'**
  String get shotTipRecordSelection;

  /// Screenshot destination (Options menu)
  ///
  /// In en, this message translates to:
  /// **'Desktop'**
  String get shotDesktop;

  /// Screenshot destination (Options menu)
  ///
  /// In en, this message translates to:
  /// **'Documents'**
  String get shotDocuments;

  /// Screenshot destination (Options menu)
  ///
  /// In en, this message translates to:
  /// **'Clipboard'**
  String get shotClipboard;

  /// Screenshot destination: choose a folder (Options menu)
  ///
  /// In en, this message translates to:
  /// **'Other Folder…'**
  String get shotOtherFolder;

  /// Title of the folder chooser for screenshots
  ///
  /// In en, this message translates to:
  /// **'Choose where screenshots go'**
  String get shotChooseFolder;

  /// Screenshot option: the bar reopens with the last selection
  ///
  /// In en, this message translates to:
  /// **'Remember Last Selection'**
  String get shotKeepSelection;

  /// Screenshot option: draw the mouse pointer into captures
  ///
  /// In en, this message translates to:
  /// **'Show Mouse Pointer'**
  String get shotIncludePointer;

  /// Screenshot options section (capture modes): what goes into the picture
  ///
  /// In en, this message translates to:
  /// **'Capture'**
  String get shotCaptureSection;

  /// Screenshot thumbnail menu
  ///
  /// In en, this message translates to:
  /// **'Save to Desktop'**
  String get shotSaveToDesktop;

  /// Screenshot thumbnail menu
  ///
  /// In en, this message translates to:
  /// **'Save to Documents'**
  String get shotSaveToDocuments;

  /// Screenshot thumbnail menu
  ///
  /// In en, this message translates to:
  /// **'Save to Clipboard'**
  String get shotSaveToClipboard;

  /// Screenshot thumbnail menu: the default image viewer
  ///
  /// In en, this message translates to:
  /// **'Open in {app}'**
  String shotOpenWith(String app);

  /// Screenshot thumbnail menu: the file manager
  ///
  /// In en, this message translates to:
  /// **'Show in {app}'**
  String shotShowIn(String app);

  /// Screenshot thumbnail menu
  ///
  /// In en, this message translates to:
  /// **'Delete'**
  String get shotDelete;

  /// Screenshot thumbnail menu: saves and closes the thumbnail
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get shotClose;

  /// Inline completion in the Faro field for a fallback search row
  ///
  /// In en, this message translates to:
  /// **'— {action}'**
  String faroCompletionAction(String action);

  /// Faro fallback row (file manager) and the hint on a chosen folder row
  ///
  /// In en, this message translates to:
  /// **'Search in {name}'**
  String faroSearchIn(String name);

  /// Faro actions scope footer when every action is visible
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =1{1 action} other{{count} actions}}'**
  String faroActionsFooter(int count);

  /// Faro actions scope footer when the list scrolls
  ///
  /// In en, this message translates to:
  /// **'{shown} of {total} actions · scroll for more'**
  String faroActionsFooterMore(int shown, int total);

  /// Launcher view menu: the grid of icons (menu item)
  ///
  /// In en, this message translates to:
  /// **'as Grid'**
  String get launcherViewGrid;

  /// Launcher view menu: the list of rows (menu item)
  ///
  /// In en, this message translates to:
  /// **'as List'**
  String get launcherViewList;

  /// Faro fallback row that searches the web
  ///
  /// In en, this message translates to:
  /// **'Search the Web'**
  String get faroSearchWeb;

  /// Faro: name of the file search when no file manager is installed
  ///
  /// In en, this message translates to:
  /// **'Files'**
  String get faroFiles;

  /// Faro folder row subtitle: modified today
  ///
  /// In en, this message translates to:
  /// **'today, {time}'**
  String faroToday(String time);

  /// Faro folder row subtitle: modified yesterday
  ///
  /// In en, this message translates to:
  /// **'yesterday, {time}'**
  String faroYesterday(String time);

  /// Name of the Tab key in Faro's key capsule
  ///
  /// In en, this message translates to:
  /// **'Tab'**
  String get faroTabKey;

  /// Accessibility label of the copy button in Faro's answer card
  ///
  /// In en, this message translates to:
  /// **'Copy'**
  String get faroCopy;

  /// Logo menu: first item while Option is held
  ///
  /// In en, this message translates to:
  /// **'System Information'**
  String get systemInformation;

  /// Logo menu: submenu of recent apps and documents
  ///
  /// In en, this message translates to:
  /// **'Recent Items'**
  String get recentItems;

  /// Recent Items submenu: section of apps
  ///
  /// In en, this message translates to:
  /// **'Applications'**
  String get recentApplications;

  /// Recent Items submenu: section of documents
  ///
  /// In en, this message translates to:
  /// **'Documents'**
  String get recentDocuments;

  /// Recent Items submenu: an empty section
  ///
  /// In en, this message translates to:
  /// **'None'**
  String get recentNone;

  /// Recent Items submenu: forget the recent items
  ///
  /// In en, this message translates to:
  /// **'Clear Menu'**
  String get recentClear;

  /// Logo menu: opens the force quit panel
  ///
  /// In en, this message translates to:
  /// **'Force Quit…'**
  String get forceQuitMenu;

  /// Logo menu: suspend the computer
  ///
  /// In en, this message translates to:
  /// **'Sleep'**
  String get sleep;

  /// Logo menu: restart with confirmation
  ///
  /// In en, this message translates to:
  /// **'Restart…'**
  String get restart;

  /// Logo menu: restart without confirmation (Option held)
  ///
  /// In en, this message translates to:
  /// **'Restart'**
  String get restartNow;

  /// Logo menu: shut down with confirmation
  ///
  /// In en, this message translates to:
  /// **'Shut Down…'**
  String get shutDown;

  /// Logo menu: shut down without confirmation (Option held)
  ///
  /// In en, this message translates to:
  /// **'Shut Down'**
  String get shutDownNow;

  /// Logo menu: lock the screen
  ///
  /// In en, this message translates to:
  /// **'Lock Screen'**
  String get lockScreen;

  /// Logo menu: log out, with the user's full name
  ///
  /// In en, this message translates to:
  /// **'Log Out {user}…'**
  String logOutUser(String user);

  /// Logo menu: log out without confirmation (Option held)
  ///
  /// In en, this message translates to:
  /// **'Log Out {user}'**
  String logOutUserNow(String user);

  /// Control Center: Focus module and detail title
  ///
  /// In en, this message translates to:
  /// **'Focus'**
  String get ccFocus;

  /// Control Center Focus detail: subtitle while a Focus is on
  ///
  /// In en, this message translates to:
  /// **'Focus on'**
  String get ccFocusOn;

  /// Control Center: accessibility label of the Dark Mode toggle
  ///
  /// In en, this message translates to:
  /// **'Dark Mode'**
  String get ccDarkMode;

  /// Control Center: accessibility label of the Screen Capture toggle
  ///
  /// In en, this message translates to:
  /// **'Screen Capture'**
  String get ccScreenCapture;

  /// Control Center Focus detail: keep Do Not Disturb on for an hour
  ///
  /// In en, this message translates to:
  /// **'For 1 hour'**
  String get focusForAnHour;

  /// Control Center Focus detail: keep Do Not Disturb on until the evening
  ///
  /// In en, this message translates to:
  /// **'Until this evening'**
  String get focusUntilEvening;

  /// Desktop context menu
  ///
  /// In en, this message translates to:
  /// **'New Folder'**
  String get desktopNewFolder;

  /// Desktop context menu
  ///
  /// In en, this message translates to:
  /// **'Paste Item'**
  String get desktopPaste;

  /// Desktop context menu
  ///
  /// In en, this message translates to:
  /// **'Get Info'**
  String get desktopGetInfo;

  /// Desktop context menu: opens the wallpaper settings
  ///
  /// In en, this message translates to:
  /// **'Change Wallpaper…'**
  String get desktopChangeWallpaper;

  /// Desktop context menu
  ///
  /// In en, this message translates to:
  /// **'Edit Widgets…'**
  String get desktopEditWidgets;

  /// Desktop context menu
  ///
  /// In en, this message translates to:
  /// **'Use Stacks'**
  String get desktopUseStacks;

  /// Desktop context menu: submenu
  ///
  /// In en, this message translates to:
  /// **'Sort By'**
  String get desktopSortBy;

  /// Desktop context menu › Sort By
  ///
  /// In en, this message translates to:
  /// **'None'**
  String get desktopSortNone;

  /// Desktop context menu › Sort By
  ///
  /// In en, this message translates to:
  /// **'Name'**
  String get desktopSortName;

  /// Desktop context menu › Sort By
  ///
  /// In en, this message translates to:
  /// **'Kind'**
  String get desktopSortKind;

  /// Desktop context menu › Sort By
  ///
  /// In en, this message translates to:
  /// **'Date Modified'**
  String get desktopSortModified;

  /// Desktop context menu
  ///
  /// In en, this message translates to:
  /// **'Show View Options'**
  String get desktopViewOptions;

  /// Name of a folder created from the desktop context menu
  ///
  /// In en, this message translates to:
  /// **'untitled folder'**
  String get desktopUntitledFolder;

  /// Desktop item context menu
  ///
  /// In en, this message translates to:
  /// **'Open'**
  String get desktopOpen;

  /// Desktop item context menu
  ///
  /// In en, this message translates to:
  /// **'Move to Trash'**
  String get desktopMoveToTrash;

  /// Desktop item context menu
  ///
  /// In en, this message translates to:
  /// **'Rename'**
  String get desktopRename;

  /// Desktop item context menu
  ///
  /// In en, this message translates to:
  /// **'Duplicate'**
  String get desktopDuplicate;

  /// Desktop item context menu (not available yet)
  ///
  /// In en, this message translates to:
  /// **'Quick Preview'**
  String get desktopQuickPreview;

  /// Desktop item context menu, on one folder: Get Info on its Customize Folder section (as the file manager's window menu)
  ///
  /// In en, this message translates to:
  /// **'Customize Folder…'**
  String get desktopCustomizeFolder;

  /// Desktop item context menu, when the selection is not one folder (not available yet)
  ///
  /// In en, this message translates to:
  /// **'Tags…'**
  String get desktopTags;

  /// Desktop context menu › Sort By
  ///
  /// In en, this message translates to:
  /// **'Size'**
  String get desktopSortSize;

  /// Desktop context menu, with Sort By › None
  ///
  /// In en, this message translates to:
  /// **'Clean Up'**
  String get desktopCleanUp;

  /// Desktop context menu: lays every icon out in an order
  ///
  /// In en, this message translates to:
  /// **'Clean Up By'**
  String get desktopCleanUpBy;

  /// Control Center Display detail: warmer colors at night
  ///
  /// In en, this message translates to:
  /// **'Night Light'**
  String get ccNightLight;

  /// Notification Center footer or empty state.
  ///
  /// In en, this message translates to:
  /// **'Choose Widgets'**
  String get chooseWidgets;

  /// Notification Center footer or empty state.
  ///
  /// In en, this message translates to:
  /// **'Clear All Notifications'**
  String get clearAllNotifications;

  /// Placeholder of a notification's reply field.
  ///
  /// In en, this message translates to:
  /// **'Reply'**
  String get replyPlaceholder;

  /// Power confirmation card (SH-MB-05)
  ///
  /// In en, this message translates to:
  /// **'Restart your computer now?'**
  String get powerRestartTitle;

  /// Power confirmation card (SH-MB-05)
  ///
  /// In en, this message translates to:
  /// **'Silica will restart automatically in {seconds} seconds.'**
  String powerRestartBody(int seconds);

  /// Power confirmation card (SH-MB-05)
  ///
  /// In en, this message translates to:
  /// **'Restart'**
  String get powerRestartButton;

  /// Power confirmation card (SH-MB-05)
  ///
  /// In en, this message translates to:
  /// **'Shut down your computer now?'**
  String get powerShutDownTitle;

  /// Power confirmation card (SH-MB-05)
  ///
  /// In en, this message translates to:
  /// **'Silica will shut down automatically in {seconds} seconds.'**
  String powerShutDownBody(int seconds);

  /// Power confirmation card (SH-MB-05)
  ///
  /// In en, this message translates to:
  /// **'Shut Down'**
  String get powerShutDownButton;

  /// Power confirmation card (SH-MB-05)
  ///
  /// In en, this message translates to:
  /// **'Log out {user} and quit all apps?'**
  String powerLogOutTitle(String user);

  /// Power confirmation card (SH-MB-05)
  ///
  /// In en, this message translates to:
  /// **'You will be logged out automatically in {seconds} seconds.'**
  String powerLogOutBody(int seconds);

  /// Power confirmation card (SH-MB-05)
  ///
  /// In en, this message translates to:
  /// **'Log Out'**
  String get powerLogOutButton;

  /// Power confirmation card (SH-MB-05)
  ///
  /// In en, this message translates to:
  /// **'Reopen windows when logging back in'**
  String get powerReopenWindows;

  /// Battery status menu: section title
  ///
  /// In en, this message translates to:
  /// **'Battery'**
  String get batteryMenuTitle;

  /// Battery status menu: the charge
  ///
  /// In en, this message translates to:
  /// **'{percent} % charged'**
  String batteryMenuCharge(int percent);

  /// Battery status menu: running on battery
  ///
  /// In en, this message translates to:
  /// **'Power Source: Battery'**
  String get batteryMenuOnBattery;

  /// Battery status menu: plugged in
  ///
  /// In en, this message translates to:
  /// **'Power Source: Power Adapter'**
  String get batteryMenuPowerAdapter;

  /// Battery status menu: opens Settings
  ///
  /// In en, this message translates to:
  /// **'Battery Settings…'**
  String get batteryMenuSettings;

  /// Help menu: opens Silica's documentation
  ///
  /// In en, this message translates to:
  /// **'Silica Help'**
  String get silicaHelp;

  /// Control Center module: sending files to nearby devices
  ///
  /// In en, this message translates to:
  /// **'Nearby Sharing'**
  String get ccNearbySharing;

  /// Control Center: a module Silica cannot drive yet
  ///
  /// In en, this message translates to:
  /// **'Not available yet'**
  String get ccNotAvailableYet;

  /// Control Center tile: arranging windows in a stage with the others at the side
  ///
  /// In en, this message translates to:
  /// **'Stage View'**
  String get ccStageView;

  /// Control Center tile: showing the screen on another display
  ///
  /// In en, this message translates to:
  /// **'Screen Mirroring'**
  String get ccScreenMirroring;

  /// Control Center button under the modules: rearranging them
  ///
  /// In en, this message translates to:
  /// **'Edit Controls'**
  String get ccEditControls;

  /// Control Center gallery: closes edit mode
  ///
  /// In en, this message translates to:
  /// **'Done'**
  String get ccDone;

  /// Control Center gallery: search field placeholder
  ///
  /// In en, this message translates to:
  /// **'Search Controls'**
  String get ccSearchControls;

  /// Control Center gallery: category with every control
  ///
  /// In en, this message translates to:
  /// **'All Controls'**
  String get ccAllControls;

  /// Control Center gallery category
  ///
  /// In en, this message translates to:
  /// **'Connectivity'**
  String get ccCategoryConnectivity;

  /// Control Center gallery category
  ///
  /// In en, this message translates to:
  /// **'Display & Brightness'**
  String get ccCategoryDisplay;

  /// Control Center gallery category
  ///
  /// In en, this message translates to:
  /// **'Desktop & Windows'**
  String get ccCategoryDesktop;

  /// Control Center module and gallery category: the media being played
  ///
  /// In en, this message translates to:
  /// **'Now Playing'**
  String get ccNowPlaying;

  /// Control Center gallery footer
  ///
  /// In en, this message translates to:
  /// **'Drag a control into Control Center to add it, or click it.'**
  String get ccGalleryHint;

  /// Control Center gallery: a control already added
  ///
  /// In en, this message translates to:
  /// **'In Control Center'**
  String get ccInCenter;

  /// Control Center module menu: the size group
  ///
  /// In en, this message translates to:
  /// **'Size'**
  String get ccSize;

  /// Control Center module menu: size
  ///
  /// In en, this message translates to:
  /// **'Small'**
  String get ccSizeSmall;

  /// Control Center module menu: size
  ///
  /// In en, this message translates to:
  /// **'Medium'**
  String get ccSizeMedium;

  /// Control Center module menu: size
  ///
  /// In en, this message translates to:
  /// **'Large'**
  String get ccSizeLarge;

  /// Control Center module menu and edit badge: removes the module
  ///
  /// In en, this message translates to:
  /// **'Remove'**
  String get ccRemove;

  /// Control Center module menu: also shows the control in the menu bar
  ///
  /// In en, this message translates to:
  /// **'Copy to Menu Bar'**
  String get ccCopyToMenuBar;

  /// Control Center module menu: hides the control's status item again
  ///
  /// In en, this message translates to:
  /// **'Remove from Menu Bar'**
  String get ccRemoveFromMenuBar;

  /// Control Center detail: opens Settings at Wi-Fi
  ///
  /// In en, this message translates to:
  /// **'Wi-Fi Settings…'**
  String get ccWifiSettings;

  /// Control Center detail: opens Settings at Bluetooth
  ///
  /// In en, this message translates to:
  /// **'Bluetooth Settings…'**
  String get ccBluetoothSettings;

  /// Control Center detail: opens Settings at Sound
  ///
  /// In en, this message translates to:
  /// **'Sound Settings…'**
  String get ccSoundSettings;

  /// Control Center detail: opens Settings at Displays
  ///
  /// In en, this message translates to:
  /// **'Displays Settings…'**
  String get ccDisplaySettings;

  /// Control Center detail: opens Settings at Focus
  ///
  /// In en, this message translates to:
  /// **'Focus Settings…'**
  String get ccFocusSettings;

  /// Control Center Wi-Fi detail: the connected network is not secured
  ///
  /// In en, this message translates to:
  /// **'Privacy Warning'**
  String get ccPrivacyWarning;

  /// Control Center privacy pill: an app used the camera, microphone or screen recently
  ///
  /// In en, this message translates to:
  /// **'{app} recently'**
  String ccPrivacyRecent(String app);

  /// Control Center privacy pill: an app is using the camera, microphone or screen
  ///
  /// In en, this message translates to:
  /// **'{app}'**
  String ccPrivacyNow(String app);

  /// Control Center module and HUD title: the keyboard backlight
  ///
  /// In en, this message translates to:
  /// **'Keyboard Brightness'**
  String get ccKeyboardBrightness;

  /// Dock separator menu: hide the Dock automatically
  ///
  /// In en, this message translates to:
  /// **'Turn hiding on'**
  String get dockHidingOn;

  /// Dock separator menu
  ///
  /// In en, this message translates to:
  /// **'Turn hiding off'**
  String get dockHidingOff;

  /// Dock separator menu, submenu
  ///
  /// In en, this message translates to:
  /// **'Position on screen'**
  String get dockPositionOnScreen;

  /// Dock position
  ///
  /// In en, this message translates to:
  /// **'Left'**
  String get dockPositionLeft;

  /// Dock position
  ///
  /// In en, this message translates to:
  /// **'Bottom'**
  String get dockPositionBottom;

  /// Dock position
  ///
  /// In en, this message translates to:
  /// **'Right'**
  String get dockPositionRight;

  /// Dock separator menu, submenu
  ///
  /// In en, this message translates to:
  /// **'Minimize effect'**
  String get dockMinimizeEffect;

  /// Minimize effect
  ///
  /// In en, this message translates to:
  /// **'Genie effect'**
  String get dockEffectGenie;

  /// Minimize effect
  ///
  /// In en, this message translates to:
  /// **'Scale effect'**
  String get dockEffectScale;

  /// Dock separator menu: opens Settings
  ///
  /// In en, this message translates to:
  /// **'Dock settings…'**
  String get dockSettingsEllipsis;

  /// Menu of the Dock's launcher item: opens the launcher
  ///
  /// In en, this message translates to:
  /// **'Show Apps'**
  String get dockShowApps;

  /// File manager's Dock menu
  ///
  /// In en, this message translates to:
  /// **'New Smart Folder'**
  String get dockNewSmartFolder;

  /// File manager's Dock menu
  ///
  /// In en, this message translates to:
  /// **'Find…'**
  String get dockFind;

  /// File manager's Dock menu
  ///
  /// In en, this message translates to:
  /// **'Go to Folder…'**
  String get dockGoToFolder;

  /// File manager's Dock menu
  ///
  /// In en, this message translates to:
  /// **'Connect to Server…'**
  String get dockConnectToServer;

  /// Dock app menu: hides the app's windows
  ///
  /// In en, this message translates to:
  /// **'Hide'**
  String get dockHide;

  /// A place in the file manager's Dock menu
  ///
  /// In en, this message translates to:
  /// **'Applications'**
  String get placeApplications;

  /// A place in the file manager's Dock menu
  ///
  /// In en, this message translates to:
  /// **'Documents'**
  String get placeDocuments;

  /// A place in the file manager's Dock menu
  ///
  /// In en, this message translates to:
  /// **'Downloads'**
  String get placeDownloads;

  /// A place in the file manager's Dock menu
  ///
  /// In en, this message translates to:
  /// **'Recents'**
  String get placeRecents;

  /// A place in the file manager's Dock menu
  ///
  /// In en, this message translates to:
  /// **'Desktop'**
  String get placeDesktop;

  /// Dock app menu, with Option held
  ///
  /// In en, this message translates to:
  /// **'Hide Others'**
  String get dockHideOthers;

  /// Dock app menu, with Option held
  ///
  /// In en, this message translates to:
  /// **'Force Quit'**
  String get dockForceQuit;

  /// Dock app options: start the app at login
  ///
  /// In en, this message translates to:
  /// **'Open at Login'**
  String get dockOpenAtLogin;

  /// Dock app options: shows the app's entry in the file manager
  ///
  /// In en, this message translates to:
  /// **'Show in Mustache'**
  String get dockShowInFileManager;

  /// Dock app options (not available yet)
  ///
  /// In en, this message translates to:
  /// **'Assign to desktop'**
  String get dockAssignTo;

  /// Faro round button and token: the apps (the Launcher)
  ///
  /// In en, this message translates to:
  /// **'Applications'**
  String get faroScopeApps;

  /// Faro round button and token: files and folders
  ///
  /// In en, this message translates to:
  /// **'Files'**
  String get faroScopeFiles;

  /// Faro round button and token: system and app actions
  ///
  /// In en, this message translates to:
  /// **'Actions'**
  String get faroScopeActions;

  /// Faro round button and token: the clipboard history
  ///
  /// In en, this message translates to:
  /// **'Clipboard'**
  String get faroScopeClipboard;

  /// Faro actions scope: the section heading of the system's own actions (the apps' sections carry the app name)
  ///
  /// In en, this message translates to:
  /// **'System'**
  String get faroSectionSystem;

  /// Faro placeholder in the files scope
  ///
  /// In en, this message translates to:
  /// **'Search Files'**
  String get faroSearchFiles;

  /// Faro placeholder in the actions scope
  ///
  /// In en, this message translates to:
  /// **'Search Actions'**
  String get faroSearchActions;

  /// Faro placeholder in the clipboard scope
  ///
  /// In en, this message translates to:
  /// **'Search Clipboard'**
  String get faroSearchClipboard;

  /// Last row of Faro clipboard history
  ///
  /// In en, this message translates to:
  /// **'Clear History'**
  String get faroClearHistory;

  /// A clipboard history entry that is an image
  ///
  /// In en, this message translates to:
  /// **'Image'**
  String get faroImage;

  /// Subtitle of a system action row in Faro
  ///
  /// In en, this message translates to:
  /// **'System'**
  String get faroSystemAction;

  /// Subtitle of a clipboard history row: when it was copied
  ///
  /// In en, this message translates to:
  /// **'Copied {time}'**
  String faroCopied(String time);

  /// Faro action row: the command screenshot.screen
  ///
  /// In en, this message translates to:
  /// **'Screenshot'**
  String get faroActionScreenshotScreen;

  /// Faro action row: the command screenshot.selection
  ///
  /// In en, this message translates to:
  /// **'Screenshot of a part'**
  String get faroActionScreenshotSelection;

  /// Faro action row: the command screenshot.toolbar
  ///
  /// In en, this message translates to:
  /// **'Screenshot tools'**
  String get faroActionScreenshotToolbar;

  /// Faro action row: the command system.dark_toggle
  ///
  /// In en, this message translates to:
  /// **'Toggle appearance'**
  String get faroActionSystemDarkToggle;

  /// Faro action row: the command system.mute_toggle
  ///
  /// In en, this message translates to:
  /// **'Toggle mute'**
  String get faroActionSystemMuteToggle;

  /// Faro action row: the command system.volume_up
  ///
  /// In en, this message translates to:
  /// **'Volume up'**
  String get faroActionSystemVolumeUp;

  /// Faro action row: the command system.volume_down
  ///
  /// In en, this message translates to:
  /// **'Volume down'**
  String get faroActionSystemVolumeDown;

  /// Faro action row: the command system.brightness_up
  ///
  /// In en, this message translates to:
  /// **'Brightness up'**
  String get faroActionSystemBrightnessUp;

  /// Faro action row: the command system.brightness_down
  ///
  /// In en, this message translates to:
  /// **'Brightness down'**
  String get faroActionSystemBrightnessDown;

  /// Faro action row: the command panorama.toggle
  ///
  /// In en, this message translates to:
  /// **'Panorama'**
  String get faroActionPanoramaToggle;

  /// Faro action row: the command panorama.app
  ///
  /// In en, this message translates to:
  /// **'App windows'**
  String get faroActionPanoramaApp;

  /// Faro action row: the command desktop.show
  ///
  /// In en, this message translates to:
  /// **'Show desktop'**
  String get faroActionDesktopShow;

  /// Faro action row: the command space.add
  ///
  /// In en, this message translates to:
  /// **'New space'**
  String get faroActionSpaceAdd;

  /// Faro action row: the command space.next
  ///
  /// In en, this message translates to:
  /// **'Next space'**
  String get faroActionSpaceNext;

  /// Faro action row: the command space.previous
  ///
  /// In en, this message translates to:
  /// **'Previous space'**
  String get faroActionSpacePrevious;

  /// Faro action row: the command launcher.toggle
  ///
  /// In en, this message translates to:
  /// **'Apps'**
  String get faroActionLauncherToggle;

  /// Faro action row: the command notifications.toggle
  ///
  /// In en, this message translates to:
  /// **'Notification Center'**
  String get faroActionNotificationsToggle;

  /// Faro action row: the command controlcenter.toggle
  ///
  /// In en, this message translates to:
  /// **'Control Center'**
  String get faroActionControlcenterToggle;

  /// Faro action row: the command window.minimize
  ///
  /// In en, this message translates to:
  /// **'Minimize window'**
  String get faroActionWindowMinimize;

  /// Faro action row: the command window.fullscreen
  ///
  /// In en, this message translates to:
  /// **'Full screen'**
  String get faroActionWindowFullscreen;

  /// Faro action row: the command window.tile_left
  ///
  /// In en, this message translates to:
  /// **'Left half'**
  String get faroActionWindowTileLeft;

  /// Faro action row: the command window.tile_right
  ///
  /// In en, this message translates to:
  /// **'Right half'**
  String get faroActionWindowTileRight;

  /// Faro action row: the command window.tile_top
  ///
  /// In en, this message translates to:
  /// **'Top half'**
  String get faroActionWindowTileTop;

  /// Faro action row: the command window.tile_bottom
  ///
  /// In en, this message translates to:
  /// **'Bottom half'**
  String get faroActionWindowTileBottom;

  /// Faro action row: the command window.tile_fill
  ///
  /// In en, this message translates to:
  /// **'Fill'**
  String get faroActionWindowTileFill;

  /// Faro action row: the command window.arrange_2
  ///
  /// In en, this message translates to:
  /// **'Arrange two windows'**
  String get faroActionWindowArrange2;

  /// Faro action row: the command window.arrange_3
  ///
  /// In en, this message translates to:
  /// **'Arrange three windows'**
  String get faroActionWindowArrange3;

  /// Faro action row: the command window.arrange_4
  ///
  /// In en, this message translates to:
  /// **'Arrange four windows'**
  String get faroActionWindowArrange4;

  /// Faro action row: the command app.force_quit_panel
  ///
  /// In en, this message translates to:
  /// **'Force quit'**
  String get faroActionAppForceQuitPanel;

  /// Faro action row: the command session.quit
  ///
  /// In en, this message translates to:
  /// **'Quit the session'**
  String get faroActionSessionQuit;

  /// Authentication dialog title when the app is unknown
  ///
  /// In en, this message translates to:
  /// **'A system change needs an administrator’s permission.'**
  String get authTitle;

  /// Authentication dialog title with the app that asked
  ///
  /// In en, this message translates to:
  /// **'{app} needs an administrator’s permission.'**
  String authTitleApp(String app);

  /// Authentication dialog: what to do
  ///
  /// In en, this message translates to:
  /// **'Type the password to allow it.'**
  String get authPrompt;

  /// Authentication dialog: the check failed for another reason
  ///
  /// In en, this message translates to:
  /// **'The password could not be checked: try again.'**
  String get authUnavailable;

  /// Authentication dialog: user field placeholder
  ///
  /// In en, this message translates to:
  /// **'User name'**
  String get authUserName;

  /// Authentication dialog: password field placeholder
  ///
  /// In en, this message translates to:
  /// **'Password'**
  String get authPassword;

  /// Authentication dialog: confirm button
  ///
  /// In en, this message translates to:
  /// **'Allow'**
  String get authAllow;

  /// Desktop item context menu: the submenu with the tag colours
  ///
  /// In en, this message translates to:
  /// **'Tags'**
  String get menubarTags;

  /// Desktop item context menu > Tags: colour
  ///
  /// In en, this message translates to:
  /// **'Red'**
  String get menubarTagRed;

  /// Desktop item context menu > Tags: colour
  ///
  /// In en, this message translates to:
  /// **'Orange'**
  String get menubarTagOrange;

  /// Desktop item context menu > Tags: colour
  ///
  /// In en, this message translates to:
  /// **'Yellow'**
  String get menubarTagYellow;

  /// Desktop item context menu > Tags: colour
  ///
  /// In en, this message translates to:
  /// **'Green'**
  String get menubarTagGreen;

  /// Desktop item context menu > Tags: colour
  ///
  /// In en, this message translates to:
  /// **'Blue'**
  String get menubarTagBlue;

  /// Desktop item context menu > Tags: colour
  ///
  /// In en, this message translates to:
  /// **'Purple'**
  String get menubarTagPurple;

  /// Desktop item context menu > Tags: colour
  ///
  /// In en, this message translates to:
  /// **'Gray'**
  String get menubarTagGray;

  /// Control Center, Display panel: section label above the brightness slider
  ///
  /// In en, this message translates to:
  /// **'Built-in display'**
  String get ccBuiltInDisplay;

  /// Control Center: title of the Nearby Sharing module (short, it has to fit the 140 pt module)
  ///
  /// In en, this message translates to:
  /// **'Nearby'**
  String get ccNearby;

  /// Control Center, Bluetooth panel: section label above the device rows
  ///
  /// In en, this message translates to:
  /// **'Devices'**
  String get ccDevices;

  /// Notification Centre: section label of the notifications that arrived today
  ///
  /// In en, this message translates to:
  /// **'Today'**
  String get notificationsToday;

  /// Notification Centre: section label of the older notifications
  ///
  /// In en, this message translates to:
  /// **'Earlier'**
  String get notificationsEarlier;

  /// Notification Centre: header link that clears every notification
  ///
  /// In en, this message translates to:
  /// **'Clear all'**
  String get notificationsClearAll;

  /// Notification time label for a notification that arrived less than a minute ago
  ///
  /// In en, this message translates to:
  /// **'now'**
  String get notificationsNow;

  /// No description provided for @lockPassword.
  ///
  /// In en, this message translates to:
  /// **'Enter Password'**
  String get lockPassword;

  /// No description provided for @lockIncorrect.
  ///
  /// In en, this message translates to:
  /// **'Incorrect password.'**
  String get lockIncorrect;

  /// No description provided for @lockUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The password could not be checked.'**
  String get lockUnavailable;

  /// Lock screen: screen reader label of the password field
  ///
  /// In en, this message translates to:
  /// **'Password, secure field'**
  String get lockPasswordField;

  /// Lock screen: label of the help button next to the password field
  ///
  /// In en, this message translates to:
  /// **'Show password hint'**
  String get lockHelpButton;

  /// Lock screen: label of the submit button inside the password field
  ///
  /// In en, this message translates to:
  /// **'Unlock'**
  String get lockSubmit;

  /// Lock screen: shown in place of the user when the help button is used and no hint was set
  ///
  /// In en, this message translates to:
  /// **'No password hint is set for this account.'**
  String get lockNoHint;

  /// Lock screen power button
  ///
  /// In en, this message translates to:
  /// **'Sleep'**
  String get lockSleep;

  /// Lock screen power button
  ///
  /// In en, this message translates to:
  /// **'Restart'**
  String get lockRestart;

  /// Lock screen power button
  ///
  /// In en, this message translates to:
  /// **'Shut Down'**
  String get lockShutDown;

  /// Lock screen banner: how many notifications an app has; the text is hidden
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =1{1 notification} other{{count} notifications}}'**
  String lockNotificationCount(int count);

  /// Status Bar: screen reader name of the Wi-Fi item
  ///
  /// In en, this message translates to:
  /// **'Wi-Fi'**
  String get statusWifi;

  /// Status Bar: Wi-Fi item value when Wi-Fi is off
  ///
  /// In en, this message translates to:
  /// **'off'**
  String get statusWifiOff;

  /// Status Bar: Wi-Fi item value when on but not connected
  ///
  /// In en, this message translates to:
  /// **'not connected'**
  String get statusWifiNotConnected;

  /// Status Bar: Wi-Fi item value
  ///
  /// In en, this message translates to:
  /// **'connected to {network}'**
  String statusWifiConnected(String network);

  /// Status Bar: screen reader name of the wired network item
  ///
  /// In en, this message translates to:
  /// **'Ethernet'**
  String get statusEthernet;

  /// Status Bar: screen reader name of the battery item
  ///
  /// In en, this message translates to:
  /// **'Battery'**
  String get statusBattery;

  /// Status Bar: battery item value
  ///
  /// In en, this message translates to:
  /// **'{percent} percent'**
  String statusBatteryLevel(int percent);

  /// Status Bar: battery item value while charging
  ///
  /// In en, this message translates to:
  /// **'{percent} percent, charging'**
  String statusBatteryCharging(int percent);

  /// Status Bar: screen reader name of the input source item
  ///
  /// In en, this message translates to:
  /// **'Input source'**
  String get statusInputSource;

  /// Input source menu of the menu bar: opens Settings
  ///
  /// In en, this message translates to:
  /// **'Keyboard Settings…'**
  String get statusInputSettings;

  /// Status Bar: screen reader name of the Bluetooth item
  ///
  /// In en, this message translates to:
  /// **'Bluetooth'**
  String get statusBluetooth;

  /// Status Bar: screen reader name of the sound item
  ///
  /// In en, this message translates to:
  /// **'Sound'**
  String get statusSound;

  /// Status Bar: screen reader name of the search (Faro) item
  ///
  /// In en, this message translates to:
  /// **'Search'**
  String get statusSearch;

  /// Status Bar: screen reader name of the clock item
  ///
  /// In en, this message translates to:
  /// **'Clock'**
  String get statusClock;

  /// Status Bar: screen reader name of the Control Center item
  ///
  /// In en, this message translates to:
  /// **'Control Center'**
  String get statusControlCenter;

  /// Status Bar: screen reader name of the Focus item
  ///
  /// In en, this message translates to:
  /// **'Focus'**
  String get statusFocus;

  /// Status Bar: screen reader name of the media item
  ///
  /// In en, this message translates to:
  /// **'Now Playing'**
  String get statusNowPlaying;

  /// Gatekeeper: first launch of an app in quarantine
  ///
  /// In en, this message translates to:
  /// **'“{app}” is an app downloaded from the Internet. Are you sure you want to open it?'**
  String gkConfirmTitle(String app);

  /// Gatekeeper: where a quarantined app came from (a disk image)
  ///
  /// In en, this message translates to:
  /// **'It comes from the disk image “{image}”.'**
  String gkFromImage(String image);

  /// Gatekeeper: where a quarantined app came from (a website)
  ///
  /// In en, this message translates to:
  /// **'It was downloaded from {site}.'**
  String gkFromSite(String site);

  /// Gatekeeper: the app that saved a quarantined app
  ///
  /// In en, this message translates to:
  /// **'It was saved by {agent}.'**
  String gkFromApp(String agent);

  /// Gatekeeper: an identified developer
  ///
  /// In en, this message translates to:
  /// **'Developer: {developer} (identified).'**
  String gkDeveloperVerified(String developer);

  /// Gatekeeper: a developer Silica does not know
  ///
  /// In en, this message translates to:
  /// **'Developer: {developer} (not identified).'**
  String gkDeveloperSelfSigned(String developer);

  /// Gatekeeper: an app that was blocked
  ///
  /// In en, this message translates to:
  /// **'“{app}” Not Opened'**
  String gkBlockTitle(String app);

  /// Gatekeeper: blocked because unsigned
  ///
  /// In en, this message translates to:
  /// **'Silica cannot verify that “{app}” is free of malware: it is not signed by its developer.'**
  String gkBlockUnsigned(String app);

  /// Gatekeeper: blocked because the developer is unknown
  ///
  /// In en, this message translates to:
  /// **'Silica cannot verify that “{app}” is free of malware: its developer is not identified.'**
  String gkBlockSelfSigned(String app);

  /// Gatekeeper: blocked because the files changed
  ///
  /// In en, this message translates to:
  /// **'“{app}” was modified after its developer signed it, or it is damaged.'**
  String gkBlockTampered(String app);

  /// Gatekeeper: blocked because it is not a valid bundle
  ///
  /// In en, this message translates to:
  /// **'“{app}” is not a valid app.'**
  String gkBlockInvalid(String app);

  /// Gatekeeper: where Open Anyway is
  ///
  /// In en, this message translates to:
  /// **'If you trust it, you can open it from Privacy & Security in Settings.'**
  String get gkBlockHint;

  /// Gatekeeper: an app whose developer key was revoked
  ///
  /// In en, this message translates to:
  /// **'“{app}” will damage your computer.'**
  String gkMalwareTitle(String app);

  /// Gatekeeper: body of the revoked-key alert
  ///
  /// In en, this message translates to:
  /// **'Its developer’s key was revoked. You should move it to the Trash.'**
  String get gkMalwareBody;

  /// Gatekeeper: open the app
  ///
  /// In en, this message translates to:
  /// **'Open'**
  String get gkOpen;

  /// Gatekeeper: close the alert
  ///
  /// In en, this message translates to:
  /// **'Done'**
  String get gkDone;

  /// Gatekeeper: move the app to the Trash
  ///
  /// In en, this message translates to:
  /// **'Move to Trash'**
  String get gkMoveToTrash;
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
      <String>['en', 'it'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppLocalizationsEn();
    case 'it':
      return AppLocalizationsIt();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
