// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get ccUnavailable => 'Unavailable';

  @override
  String get ccOff => 'Off';

  @override
  String get ccOn => 'On';

  @override
  String get ccWired => 'Wired network';

  @override
  String get ccNotConnected => 'Not connected';

  @override
  String get ccDoNotDisturb => 'Do not disturb';

  @override
  String get ccDoNotDisturbOn => 'On';

  @override
  String get ccBattery => 'Battery';

  @override
  String ccBatteryLevel(int percent) {
    return '$percent%';
  }

  @override
  String ccBatteryCharging(int percent) {
    return '$percent% · charging';
  }

  @override
  String get ccDisplay => 'Display';

  @override
  String get ccSound => 'Sound';

  @override
  String get mediaUntitled => 'Untitled';

  @override
  String get mediaNothingPlaying => 'Nothing\nplaying';

  @override
  String knownNetworks(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: 'Known networks',
      one: 'Known network',
    );
    return '$_temp0';
  }

  @override
  String get otherNetworks => 'Other networks';

  @override
  String get soundOutput => 'Output';

  @override
  String get hudAudioOutput => 'Audio output';

  @override
  String get stackSortBy => 'Sort by';

  @override
  String get sortName => 'Name';

  @override
  String get sortDateAdded => 'Date added';

  @override
  String get sortDateModified => 'Date modified';

  @override
  String get sortDateCreated => 'Date created';

  @override
  String get sortKind => 'Kind';

  @override
  String get stackShowAs => 'Show as';

  @override
  String get stackFolder => 'Folder';

  @override
  String get stackStack => 'Stack';

  @override
  String get stackViewContentAs => 'View content as';

  @override
  String get stackFan => 'Fan';

  @override
  String get stackGrid => 'Grid';

  @override
  String get stackList => 'List';

  @override
  String get stackAutomatic => 'Automatic';

  @override
  String get options => 'Options';

  @override
  String get showInEnclosingFolder => 'Show in enclosing folder';

  @override
  String openNamed(String name) {
    return 'Open \"$name\"';
  }

  @override
  String get magnificationOff => 'Turn magnification off';

  @override
  String get magnificationOn => 'Turn magnification on';

  @override
  String get showRecentApps => 'Show recent apps';

  @override
  String get open => 'Open';

  @override
  String get trash => 'Trash';

  @override
  String get dockApps => 'Apps';

  @override
  String get newWindow => 'New Window';

  @override
  String get emptyTrashEllipsis => 'Empty Trash…';

  @override
  String get removeFromDock => 'Remove from Dock';

  @override
  String get keepInDock => 'Keep in Dock';

  @override
  String get showAllWindows => 'Show all windows';

  @override
  String get quit => 'Quit';

  @override
  String get dockRemove => 'Remove';

  @override
  String get dockRemoveFromDock => 'Remove from Dock';

  @override
  String stackMoreInFolder(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count more in the folder',
      one: '1 more in the folder',
    );
    return '$_temp0';
  }

  @override
  String get openFolder => 'Open folder';

  @override
  String get trashAlertTitle => 'Erase the items in the Trash for good?';

  @override
  String get trashAlertMessage => 'This cannot be undone.';

  @override
  String get emptyTrash => 'Empty Trash';

  @override
  String get cancel => 'Cancel';

  @override
  String get faroPlaceholder => 'Search with Faro';

  @override
  String get faroApplication => 'Application';

  @override
  String completionOpen(String rest) {
    return '$rest — Open';
  }

  @override
  String get launcherPlaceholder => 'Applications';

  @override
  String get catProductivity => 'Productivity and finance';

  @override
  String get catUtilities => 'Utilities';

  @override
  String get catCreativity => 'Creativity';

  @override
  String get catEntertainment => 'Entertainment';

  @override
  String get catSocial => 'Social';

  @override
  String get catReading => 'Information and reading';

  @override
  String get catOther => 'Other';

  @override
  String get forceQuitTitle => 'Quit apps';

  @override
  String get forceQuitHelp =>
      'If an app does not respond, choose it in the list and click \"Quit\".';

  @override
  String get forceQuitShortcut => 'This panel also opens with ⌘⌥Esc.';

  @override
  String get forceQuitButton => 'Quit';

  @override
  String get menuFile => 'File';

  @override
  String get menuEdit => 'Edit';

  @override
  String get menuView => 'View';

  @override
  String get menuWindow => 'Window';

  @override
  String get menuHelp => 'Help';

  @override
  String get aboutSilica => 'About Silica';

  @override
  String get systemSettings => 'Settings…';

  @override
  String get panorama => 'Panorama';

  @override
  String get showDesktop => 'Show desktop';

  @override
  String get logOut => 'Log out…';

  @override
  String aboutApp(String app) {
    return 'About $app';
  }

  @override
  String quitApp(String app) {
    return 'Quit $app';
  }

  @override
  String get closeWindow => 'Close window';

  @override
  String get closeAll => 'Close All';

  @override
  String get fullScreen => 'Full screen';

  @override
  String get minimize => 'Minimize';

  @override
  String get appWindows => 'App windows';

  @override
  String get nextSpace => 'Next space';

  @override
  String get previousSpace => 'Previous space';

  @override
  String get services => 'Services';

  @override
  String get noServices => 'No Services Available';

  @override
  String hideApp(String app) {
    return 'Hide $app';
  }

  @override
  String get hideOthers => 'Hide Others';

  @override
  String get showAll => 'Show All';

  @override
  String get fill => 'Fill';

  @override
  String get moveAndResize => 'Move & Resize';

  @override
  String get tileLeft => 'Left';

  @override
  String get tileRight => 'Right';

  @override
  String get tileTop => 'Top';

  @override
  String get tileBottom => 'Bottom';

  @override
  String get arrangeLeftRight => 'Left & Right';

  @override
  String get arrangeLeftQuarters => 'Left & Quarters';

  @override
  String get arrangeQuarters => 'Quarters';

  @override
  String get bringAllToFront => 'Bring All to Front';

  @override
  String appHelp(String app) {
    return '$app Help';
  }

  @override
  String get appSettings => 'Settings…';

  @override
  String get editUndo => 'Undo';

  @override
  String get editRedo => 'Redo';

  @override
  String get editCut => 'Cut';

  @override
  String get editCopy => 'Copy';

  @override
  String get editPaste => 'Paste';

  @override
  String get editDelete => 'Delete';

  @override
  String get editSelectAll => 'Select All';

  @override
  String get halfScreen => 'Half of the screen';

  @override
  String get arrange => 'Arrange';

  @override
  String get notificationsTitle => 'Notifications';

  @override
  String get noNotifications => 'No Recent Notifications';

  @override
  String get collapse => 'Collapse';

  @override
  String get clear => 'Clear';

  @override
  String get details => 'Details';

  @override
  String screenshotFileName(String date, String time) {
    return 'Screenshot $date at $time';
  }

  @override
  String get saveTo => 'Save to';

  @override
  String get delay => 'Timer';

  @override
  String get delayNone => 'None';

  @override
  String delaySeconds(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count seconds',
      one: '1 second',
    );
    return '$_temp0';
  }

  @override
  String get thumbnailAfterCapture => 'Show Floating Thumbnail';

  @override
  String get optionsButton => 'Options ⌄';

  @override
  String get capture => 'Capture';

  @override
  String get shotTipCaptureScreen => 'Capture Entire Screen';

  @override
  String get shotTipCaptureWindow => 'Capture Selected Window';

  @override
  String get shotTipCaptureSelection => 'Capture Selected Portion';

  @override
  String get shotTipRecordScreen => 'Record Entire Screen';

  @override
  String get shotTipRecordWindow => 'Record Selected Window';

  @override
  String get shotTipRecordSelection => 'Record Selected Portion';

  @override
  String get shotDesktop => 'Desktop';

  @override
  String get shotDocuments => 'Documents';

  @override
  String get shotClipboard => 'Clipboard';

  @override
  String get shotOtherFolder => 'Other Folder…';

  @override
  String get shotChooseFolder => 'Choose where screenshots go';

  @override
  String get shotKeepSelection => 'Remember Last Selection';

  @override
  String get shotIncludePointer => 'Show Mouse Pointer';

  @override
  String get shotCaptureSection => 'Capture';

  @override
  String get shotSaveToDesktop => 'Save to Desktop';

  @override
  String get shotSaveToDocuments => 'Save to Documents';

  @override
  String get shotSaveToClipboard => 'Save to Clipboard';

  @override
  String shotOpenWith(String app) {
    return 'Open in $app';
  }

  @override
  String shotShowIn(String app) {
    return 'Show in $app';
  }

  @override
  String get shotDelete => 'Delete';

  @override
  String get shotClose => 'Close';

  @override
  String faroCompletionAction(String action) {
    return '— $action';
  }

  @override
  String faroSearchIn(String name) {
    return 'Search in $name';
  }

  @override
  String faroActionsFooter(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count actions',
      one: '1 action',
    );
    return '$_temp0';
  }

  @override
  String faroActionsFooterMore(int shown, int total) {
    return '$shown of $total actions · scroll for more';
  }

  @override
  String get launcherViewGrid => 'as Grid';

  @override
  String get launcherViewList => 'as List';

  @override
  String get faroSearchWeb => 'Search the Web';

  @override
  String get faroFiles => 'Files';

  @override
  String faroToday(String time) {
    return 'today, $time';
  }

  @override
  String faroYesterday(String time) {
    return 'yesterday, $time';
  }

  @override
  String get faroTabKey => 'Tab';

  @override
  String get faroCopy => 'Copy';

  @override
  String get systemInformation => 'System Information';

  @override
  String get recentItems => 'Recent Items';

  @override
  String get recentApplications => 'Applications';

  @override
  String get recentDocuments => 'Documents';

  @override
  String get recentNone => 'None';

  @override
  String get recentClear => 'Clear Menu';

  @override
  String get forceQuitMenu => 'Force Quit…';

  @override
  String get sleep => 'Sleep';

  @override
  String get restart => 'Restart…';

  @override
  String get restartNow => 'Restart';

  @override
  String get shutDown => 'Shut Down…';

  @override
  String get shutDownNow => 'Shut Down';

  @override
  String get lockScreen => 'Lock Screen';

  @override
  String logOutUser(String user) {
    return 'Log Out $user…';
  }

  @override
  String logOutUserNow(String user) {
    return 'Log Out $user';
  }

  @override
  String get ccFocus => 'Focus';

  @override
  String get ccFocusOn => 'Focus on';

  @override
  String get ccDarkMode => 'Dark Mode';

  @override
  String get ccScreenCapture => 'Screen Capture';

  @override
  String get focusForAnHour => 'For 1 hour';

  @override
  String get focusUntilEvening => 'Until this evening';

  @override
  String get desktopNewFolder => 'New Folder';

  @override
  String get desktopPaste => 'Paste Item';

  @override
  String get desktopGetInfo => 'Get Info';

  @override
  String get desktopChangeWallpaper => 'Change Wallpaper…';

  @override
  String get desktopEditWidgets => 'Edit Widgets…';

  @override
  String get desktopUseStacks => 'Use Stacks';

  @override
  String get desktopSortBy => 'Sort By';

  @override
  String get desktopSortNone => 'None';

  @override
  String get desktopSortName => 'Name';

  @override
  String get desktopSortKind => 'Kind';

  @override
  String get desktopSortModified => 'Date Modified';

  @override
  String get desktopViewOptions => 'Show View Options';

  @override
  String get desktopUntitledFolder => 'untitled folder';

  @override
  String get desktopOpen => 'Open';

  @override
  String get desktopMoveToTrash => 'Move to Trash';

  @override
  String get desktopRename => 'Rename';

  @override
  String get desktopDuplicate => 'Duplicate';

  @override
  String get desktopQuickPreview => 'Quick Preview';

  @override
  String get desktopCustomizeFolder => 'Customize Folder…';

  @override
  String get desktopTags => 'Tags…';

  @override
  String get desktopSortSize => 'Size';

  @override
  String get desktopCleanUp => 'Clean Up';

  @override
  String get desktopCleanUpBy => 'Clean Up By';

  @override
  String get ccNightLight => 'Night Light';

  @override
  String get chooseWidgets => 'Choose Widgets';

  @override
  String get clearAllNotifications => 'Clear All Notifications';

  @override
  String get replyPlaceholder => 'Reply';

  @override
  String get powerRestartTitle => 'Restart your computer now?';

  @override
  String powerRestartBody(int seconds) {
    return 'Silica will restart automatically in $seconds seconds.';
  }

  @override
  String get powerRestartButton => 'Restart';

  @override
  String get powerShutDownTitle => 'Shut down your computer now?';

  @override
  String powerShutDownBody(int seconds) {
    return 'Silica will shut down automatically in $seconds seconds.';
  }

  @override
  String get powerShutDownButton => 'Shut Down';

  @override
  String powerLogOutTitle(String user) {
    return 'Log out $user and quit all apps?';
  }

  @override
  String powerLogOutBody(int seconds) {
    return 'You will be logged out automatically in $seconds seconds.';
  }

  @override
  String get powerLogOutButton => 'Log Out';

  @override
  String get powerReopenWindows => 'Reopen windows when logging back in';

  @override
  String get batteryMenuTitle => 'Battery';

  @override
  String batteryMenuCharge(int percent) {
    return '$percent % charged';
  }

  @override
  String get batteryMenuOnBattery => 'Power Source: Battery';

  @override
  String get batteryMenuPowerAdapter => 'Power Source: Power Adapter';

  @override
  String get batteryMenuSettings => 'Battery Settings…';

  @override
  String get silicaHelp => 'Silica Help';

  @override
  String get ccNearbySharing => 'Nearby Sharing';

  @override
  String get ccNotAvailableYet => 'Not available yet';

  @override
  String get ccStageView => 'Stage View';

  @override
  String get ccScreenMirroring => 'Screen Mirroring';

  @override
  String get ccEditControls => 'Edit Controls';

  @override
  String get ccDone => 'Done';

  @override
  String get ccSearchControls => 'Search Controls';

  @override
  String get ccAllControls => 'All Controls';

  @override
  String get ccCategoryConnectivity => 'Connectivity';

  @override
  String get ccCategoryDisplay => 'Display & Brightness';

  @override
  String get ccCategoryDesktop => 'Desktop & Windows';

  @override
  String get ccNowPlaying => 'Now Playing';

  @override
  String get ccGalleryHint =>
      'Drag a control into Control Center to add it, or click it.';

  @override
  String get ccInCenter => 'In Control Center';

  @override
  String get ccSize => 'Size';

  @override
  String get ccSizeSmall => 'Small';

  @override
  String get ccSizeMedium => 'Medium';

  @override
  String get ccSizeLarge => 'Large';

  @override
  String get ccRemove => 'Remove';

  @override
  String get ccCopyToMenuBar => 'Copy to Menu Bar';

  @override
  String get ccRemoveFromMenuBar => 'Remove from Menu Bar';

  @override
  String get ccWifiSettings => 'Wi-Fi Settings…';

  @override
  String get ccBluetoothSettings => 'Bluetooth Settings…';

  @override
  String get ccSoundSettings => 'Sound Settings…';

  @override
  String get ccDisplaySettings => 'Displays Settings…';

  @override
  String get ccFocusSettings => 'Focus Settings…';

  @override
  String get ccPrivacyWarning => 'Privacy Warning';

  @override
  String ccPrivacyRecent(String app) {
    return '$app recently';
  }

  @override
  String ccPrivacyNow(String app) {
    return '$app';
  }

  @override
  String get ccKeyboardBrightness => 'Keyboard Brightness';

  @override
  String get dockHidingOn => 'Turn hiding on';

  @override
  String get dockHidingOff => 'Turn hiding off';

  @override
  String get dockPositionOnScreen => 'Position on screen';

  @override
  String get dockPositionLeft => 'Left';

  @override
  String get dockPositionBottom => 'Bottom';

  @override
  String get dockPositionRight => 'Right';

  @override
  String get dockMinimizeEffect => 'Minimize effect';

  @override
  String get dockEffectGenie => 'Genie effect';

  @override
  String get dockEffectScale => 'Scale effect';

  @override
  String get dockSettingsEllipsis => 'Dock settings…';

  @override
  String get dockShowApps => 'Show Apps';

  @override
  String get dockNewSmartFolder => 'New Smart Folder';

  @override
  String get dockFind => 'Find…';

  @override
  String get dockGoToFolder => 'Go to Folder…';

  @override
  String get dockConnectToServer => 'Connect to Server…';

  @override
  String get dockHide => 'Hide';

  @override
  String get placeApplications => 'Applications';

  @override
  String get placeDocuments => 'Documents';

  @override
  String get placeDownloads => 'Downloads';

  @override
  String get placeRecents => 'Recents';

  @override
  String get placeDesktop => 'Desktop';

  @override
  String get dockHideOthers => 'Hide Others';

  @override
  String get dockForceQuit => 'Force Quit';

  @override
  String get dockOpenAtLogin => 'Open at Login';

  @override
  String get dockShowInFileManager => 'Show in Mustache';

  @override
  String get dockAssignTo => 'Assign to desktop';

  @override
  String get faroScopeApps => 'Applications';

  @override
  String get faroScopeFiles => 'Files';

  @override
  String get faroScopeActions => 'Actions';

  @override
  String get faroScopeClipboard => 'Clipboard';

  @override
  String get faroSectionSystem => 'System';

  @override
  String get faroSearchFiles => 'Search Files';

  @override
  String get faroSearchActions => 'Search Actions';

  @override
  String get faroSearchClipboard => 'Search Clipboard';

  @override
  String get faroClearHistory => 'Clear History';

  @override
  String get faroImage => 'Image';

  @override
  String get faroSystemAction => 'System';

  @override
  String faroCopied(String time) {
    return 'Copied $time';
  }

  @override
  String get faroActionScreenshotScreen => 'Screenshot';

  @override
  String get faroActionScreenshotSelection => 'Screenshot of a part';

  @override
  String get faroActionScreenshotToolbar => 'Screenshot tools';

  @override
  String get faroActionSystemDarkToggle => 'Toggle appearance';

  @override
  String get faroActionSystemMuteToggle => 'Toggle mute';

  @override
  String get faroActionSystemVolumeUp => 'Volume up';

  @override
  String get faroActionSystemVolumeDown => 'Volume down';

  @override
  String get faroActionSystemBrightnessUp => 'Brightness up';

  @override
  String get faroActionSystemBrightnessDown => 'Brightness down';

  @override
  String get faroActionPanoramaToggle => 'Panorama';

  @override
  String get faroActionPanoramaApp => 'App windows';

  @override
  String get faroActionDesktopShow => 'Show desktop';

  @override
  String get faroActionSpaceAdd => 'New space';

  @override
  String get faroActionSpaceNext => 'Next space';

  @override
  String get faroActionSpacePrevious => 'Previous space';

  @override
  String get faroActionLauncherToggle => 'Apps';

  @override
  String get faroActionNotificationsToggle => 'Notification Center';

  @override
  String get faroActionControlcenterToggle => 'Control Center';

  @override
  String get faroActionWindowMinimize => 'Minimize window';

  @override
  String get faroActionWindowFullscreen => 'Full screen';

  @override
  String get faroActionWindowTileLeft => 'Left half';

  @override
  String get faroActionWindowTileRight => 'Right half';

  @override
  String get faroActionWindowTileTop => 'Top half';

  @override
  String get faroActionWindowTileBottom => 'Bottom half';

  @override
  String get faroActionWindowTileFill => 'Fill';

  @override
  String get faroActionWindowArrange2 => 'Arrange two windows';

  @override
  String get faroActionWindowArrange3 => 'Arrange three windows';

  @override
  String get faroActionWindowArrange4 => 'Arrange four windows';

  @override
  String get faroActionAppForceQuitPanel => 'Force quit';

  @override
  String get faroActionSessionQuit => 'Quit the session';

  @override
  String get authTitle =>
      'A system change needs an administrator’s permission.';

  @override
  String authTitleApp(String app) {
    return '$app needs an administrator’s permission.';
  }

  @override
  String get authPrompt => 'Type the password to allow it.';

  @override
  String get authUnavailable => 'The password could not be checked: try again.';

  @override
  String get authUserName => 'User name';

  @override
  String get authPassword => 'Password';

  @override
  String get authAllow => 'Allow';

  @override
  String get menubarTags => 'Tags';

  @override
  String get menubarTagRed => 'Red';

  @override
  String get menubarTagOrange => 'Orange';

  @override
  String get menubarTagYellow => 'Yellow';

  @override
  String get menubarTagGreen => 'Green';

  @override
  String get menubarTagBlue => 'Blue';

  @override
  String get menubarTagPurple => 'Purple';

  @override
  String get menubarTagGray => 'Gray';

  @override
  String get ccBuiltInDisplay => 'Built-in display';

  @override
  String get ccNearby => 'Nearby';

  @override
  String get ccDevices => 'Devices';

  @override
  String get notificationsToday => 'Today';

  @override
  String get notificationsEarlier => 'Earlier';

  @override
  String get notificationsClearAll => 'Clear all';

  @override
  String get notificationsNow => 'now';

  @override
  String get lockPassword => 'Enter Password';

  @override
  String get lockIncorrect => 'Incorrect password.';

  @override
  String get lockUnavailable => 'The password could not be checked.';

  @override
  String get lockPasswordField => 'Password, secure field';

  @override
  String get lockHelpButton => 'Show password hint';

  @override
  String get lockSubmit => 'Unlock';

  @override
  String get lockNoHint => 'No password hint is set for this account.';

  @override
  String get lockSleep => 'Sleep';

  @override
  String get lockRestart => 'Restart';

  @override
  String get lockShutDown => 'Shut Down';

  @override
  String lockNotificationCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count notifications',
      one: '1 notification',
    );
    return '$_temp0';
  }

  @override
  String get statusWifi => 'Wi-Fi';

  @override
  String get statusWifiOff => 'off';

  @override
  String get statusWifiNotConnected => 'not connected';

  @override
  String statusWifiConnected(String network) {
    return 'connected to $network';
  }

  @override
  String get statusEthernet => 'Ethernet';

  @override
  String get statusBattery => 'Battery';

  @override
  String statusBatteryLevel(int percent) {
    return '$percent percent';
  }

  @override
  String statusBatteryCharging(int percent) {
    return '$percent percent, charging';
  }

  @override
  String get statusInputSource => 'Input source';

  @override
  String get statusInputSettings => 'Keyboard Settings…';

  @override
  String get statusBluetooth => 'Bluetooth';

  @override
  String get statusSound => 'Sound';

  @override
  String get statusSearch => 'Search';

  @override
  String get statusClock => 'Clock';

  @override
  String get statusControlCenter => 'Control Center';

  @override
  String get statusFocus => 'Focus';

  @override
  String get statusNowPlaying => 'Now Playing';
}
