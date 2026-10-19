// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Italian (`it`).
class AppLocalizationsIt extends AppLocalizations {
  AppLocalizationsIt([String locale = 'it']) : super(locale);

  @override
  String get ccUnavailable => 'Non disponibile';

  @override
  String get ccOff => 'Spento';

  @override
  String get ccOn => 'Acceso';

  @override
  String get ccWired => 'Rete via cavo';

  @override
  String get ccNotConnected => 'Non connesso';

  @override
  String get ccDoNotDisturb => 'Non disturbare';

  @override
  String get ccDoNotDisturbOn => 'Attivo';

  @override
  String get ccBattery => 'Batteria';

  @override
  String ccBatteryLevel(int percent) {
    return '$percent%';
  }

  @override
  String ccBatteryCharging(int percent) {
    return '$percent% · in carica';
  }

  @override
  String get ccDisplay => 'Schermo';

  @override
  String get ccSound => 'Suono';

  @override
  String get mediaUntitled => 'Senza titolo';

  @override
  String get mediaNothingPlaying => 'Nessuna\nriproduzione';

  @override
  String knownNetworks(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: 'Reti conosciute',
      one: 'Rete conosciuta',
    );
    return '$_temp0';
  }

  @override
  String get otherNetworks => 'Altre reti';

  @override
  String get soundOutput => 'Uscita';

  @override
  String get hudAudioOutput => 'Uscita audio';

  @override
  String get stackSortBy => 'Ordina per';

  @override
  String get sortName => 'Nome';

  @override
  String get sortDateAdded => 'Data di aggiunta';

  @override
  String get sortDateModified => 'Data di modifica';

  @override
  String get sortDateCreated => 'Data di creazione';

  @override
  String get sortKind => 'Tipo';

  @override
  String get stackShowAs => 'Mostra come';

  @override
  String get stackFolder => 'Cartella';

  @override
  String get stackStack => 'Pila';

  @override
  String get stackViewContentAs => 'Visualizza contenuto come';

  @override
  String get stackFan => 'Ventaglio';

  @override
  String get stackGrid => 'Griglia';

  @override
  String get stackList => 'Elenco';

  @override
  String get stackAutomatic => 'Automatico';

  @override
  String get options => 'Opzioni';

  @override
  String get showInEnclosingFolder => 'Mostra nella cartella che la contiene';

  @override
  String openNamed(String name) {
    return 'Apri \"$name\"';
  }

  @override
  String get magnificationOff => 'Disattiva l\'ingrandimento';

  @override
  String get magnificationOn => 'Attiva l\'ingrandimento';

  @override
  String get showRecentApps => 'Mostra le app recenti';

  @override
  String get open => 'Apri';

  @override
  String get trash => 'Cestino';

  @override
  String get dockApps => 'App';

  @override
  String get newWindow => 'Nuova finestra';

  @override
  String get emptyTrashEllipsis => 'Svuota il Cestino…';

  @override
  String get removeFromDock => 'Rimuovi dal Dock';

  @override
  String get keepInDock => 'Mantieni nel Dock';

  @override
  String get showAllWindows => 'Mostra tutte le finestre';

  @override
  String get quit => 'Esci';

  @override
  String get dockRemove => 'Rimuovi';

  @override
  String get dockRemoveFromDock => 'Rimuovi dal Dock';

  @override
  String stackMoreInFolder(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: 'Altri $count nella cartella',
    );
    return '$_temp0';
  }

  @override
  String get openFolder => 'Apri la cartella';

  @override
  String get trashAlertTitle =>
      'Vuoi eliminare definitivamente gli elementi nel Cestino?';

  @override
  String get trashAlertMessage => 'Non si potrà annullare.';

  @override
  String get emptyTrash => 'Svuota il Cestino';

  @override
  String get cancel => 'Annulla';

  @override
  String get faroPlaceholder => 'Cerca con Faro';

  @override
  String get faroApplication => 'Applicazione';

  @override
  String completionOpen(String rest) {
    return '$rest — Apri';
  }

  @override
  String get launcherPlaceholder => 'Applicazioni';

  @override
  String get catProductivity => 'Produttività e finanza';

  @override
  String get catUtilities => 'Utility';

  @override
  String get catCreativity => 'Creatività';

  @override
  String get catEntertainment => 'Svago';

  @override
  String get catSocial => 'Social';

  @override
  String get catReading => 'Informazione e lettura';

  @override
  String get catOther => 'Altro';

  @override
  String get forceQuitTitle => 'Termina le app';

  @override
  String get forceQuitHelp =>
      'Se un\'app non risponde, sceglila nell\'elenco e fai clic su «Termina».';

  @override
  String get forceQuitShortcut => 'Questo pannello si apre anche con ⌘⌥Esc.';

  @override
  String get forceQuitButton => 'Termina';

  @override
  String get menuFile => 'File';

  @override
  String get menuEdit => 'Modifica';

  @override
  String get menuView => 'Vista';

  @override
  String get menuWindow => 'Finestra';

  @override
  String get menuHelp => 'Aiuto';

  @override
  String get aboutSilica => 'Informazioni su Silica';

  @override
  String get systemSettings => 'Impostazioni di sistema…';

  @override
  String get panorama => 'Panorama';

  @override
  String get showDesktop => 'Mostra la scrivania';

  @override
  String get logOut => 'Esci dalla sessione…';

  @override
  String aboutApp(String app) {
    return 'Informazioni su $app';
  }

  @override
  String quitApp(String app) {
    return 'Esci da $app';
  }

  @override
  String get closeWindow => 'Chiudi finestra';

  @override
  String get closeAll => 'Chiudi tutto';

  @override
  String get fullScreen => 'Schermo intero';

  @override
  String get minimize => 'Riduci';

  @override
  String get appWindows => 'Finestre dell\'app';

  @override
  String get nextSpace => 'Spazio successivo';

  @override
  String get previousSpace => 'Spazio precedente';

  @override
  String get services => 'Servizi';

  @override
  String get noServices => 'Nessun servizio';

  @override
  String hideApp(String app) {
    return 'Nascondi $app';
  }

  @override
  String get hideOthers => 'Nascondi le altre';

  @override
  String get showAll => 'Mostra tutto';

  @override
  String get fill => 'Riempi';

  @override
  String get moveAndResize => 'Sposta e ridimensiona';

  @override
  String get tileLeft => 'A sinistra';

  @override
  String get tileRight => 'A destra';

  @override
  String get tileTop => 'In alto';

  @override
  String get tileBottom => 'In basso';

  @override
  String get arrangeLeftRight => 'Sinistra e destra';

  @override
  String get arrangeLeftQuarters => 'Sinistra e quarti';

  @override
  String get arrangeQuarters => 'In quarti';

  @override
  String get bringAllToFront => 'Porta tutto davanti';

  @override
  String appHelp(String app) {
    return 'Aiuto di $app';
  }

  @override
  String get appSettings => 'Impostazioni…';

  @override
  String get editUndo => 'Annulla';

  @override
  String get editRedo => 'Ripeti';

  @override
  String get editCut => 'Taglia';

  @override
  String get editCopy => 'Copia';

  @override
  String get editPaste => 'Incolla';

  @override
  String get editDelete => 'Elimina';

  @override
  String get editSelectAll => 'Seleziona tutto';

  @override
  String get halfScreen => 'Metà dello schermo';

  @override
  String get arrange => 'Disponi';

  @override
  String get notificationsTitle => 'Notifiche';

  @override
  String get noNotifications => 'Nessuna notifica recente';

  @override
  String get collapse => 'Riduci';

  @override
  String get clear => 'Cancella';

  @override
  String get details => 'Dettagli';

  @override
  String screenshotFileName(String date, String time) {
    return 'Istantanea schermo $date alle $time';
  }

  @override
  String get saveTo => 'Salva in';

  @override
  String get delay => 'Timer';

  @override
  String get delayNone => 'Nessuno';

  @override
  String delaySeconds(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count secondi',
      one: '1 secondo',
    );
    return '$_temp0';
  }

  @override
  String get thumbnailAfterCapture => 'Mostra miniatura fluttuante';

  @override
  String get optionsButton => 'Opzioni ⌄';

  @override
  String get capture => 'Acquisisci';

  @override
  String get shotTipCaptureScreen => 'Acquisisci l\'intero schermo';

  @override
  String get shotTipCaptureWindow => 'Acquisisci la finestra selezionata';

  @override
  String get shotTipCaptureSelection => 'Acquisisci la porzione selezionata';

  @override
  String get shotTipRecordScreen => 'Registra l\'intero schermo';

  @override
  String get shotTipRecordWindow => 'Registra la finestra selezionata';

  @override
  String get shotTipRecordSelection => 'Registra la porzione selezionata';

  @override
  String get shotDesktop => 'Scrivania';

  @override
  String get shotDocuments => 'Documenti';

  @override
  String get shotClipboard => 'Appunti';

  @override
  String get shotOtherFolder => 'Altra cartella…';

  @override
  String get shotChooseFolder => 'Scegli dove salvare le istantanee';

  @override
  String get shotKeepSelection => 'Ricorda ultima selezione';

  @override
  String get shotIncludePointer => 'Mostra puntatore del mouse';

  @override
  String get shotCaptureSection => 'Acquisisci';

  @override
  String get shotSaveToDesktop => 'Salva sulla Scrivania';

  @override
  String get shotSaveToDocuments => 'Salva in Documenti';

  @override
  String get shotSaveToClipboard => 'Salva negli Appunti';

  @override
  String shotOpenWith(String app) {
    return 'Apri in $app';
  }

  @override
  String shotShowIn(String app) {
    return 'Mostra in $app';
  }

  @override
  String get shotDelete => 'Elimina';

  @override
  String get shotClose => 'Chiudi';

  @override
  String faroCompletionAction(String action) {
    return '— $action';
  }

  @override
  String faroSearchIn(String name) {
    return 'Cerca in $name';
  }

  @override
  String faroActionsFooter(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count azioni',
      one: '1 azione',
    );
    return '$_temp0';
  }

  @override
  String faroActionsFooterMore(int shown, int total) {
    return '$shown di $total azioni · scorri per vedere le altre';
  }

  @override
  String get launcherViewGrid => 'come griglia';

  @override
  String get launcherViewList => 'come elenco';

  @override
  String get faroSearchWeb => 'Cerca nel web';

  @override
  String get faroFiles => 'File';

  @override
  String faroToday(String time) {
    return 'oggi, $time';
  }

  @override
  String faroYesterday(String time) {
    return 'ieri, $time';
  }

  @override
  String get faroTabKey => 'Tabulatore';

  @override
  String get faroCopy => 'Copia';

  @override
  String get systemInformation => 'Informazioni di sistema';

  @override
  String get recentItems => 'Elementi recenti';

  @override
  String get recentApplications => 'Applicazioni';

  @override
  String get recentDocuments => 'Documenti';

  @override
  String get recentNone => 'Nessuno';

  @override
  String get recentClear => 'Cancella menu';

  @override
  String get forceQuitMenu => 'Uscita forzata…';

  @override
  String get sleep => 'Standby';

  @override
  String get restart => 'Riavvia…';

  @override
  String get restartNow => 'Riavvia';

  @override
  String get shutDown => 'Spegni…';

  @override
  String get shutDownNow => 'Spegni';

  @override
  String get lockScreen => 'Blocca schermo';

  @override
  String logOutUser(String user) {
    return 'Esci da $user…';
  }

  @override
  String logOutUserNow(String user) {
    return 'Esci da $user';
  }

  @override
  String get ccFocus => 'Focus';

  @override
  String get ccFocusOn => 'Focus attivo';

  @override
  String get ccDarkMode => 'Modalità scura';

  @override
  String get ccScreenCapture => 'Acquisizione schermo';

  @override
  String get focusForAnHour => 'Per un\'ora';

  @override
  String get focusUntilEvening => 'Fino a stasera';

  @override
  String get desktopNewFolder => 'Nuova cartella';

  @override
  String get desktopPaste => 'Incolla elemento';

  @override
  String get desktopGetInfo => 'Informazioni';

  @override
  String get desktopChangeWallpaper => 'Cambia sfondo…';

  @override
  String get desktopEditWidgets => 'Modifica widget…';

  @override
  String get desktopUseStacks => 'Usa pile';

  @override
  String get desktopSortBy => 'Ordina per';

  @override
  String get desktopSortNone => 'Nessuno';

  @override
  String get desktopSortName => 'Nome';

  @override
  String get desktopSortKind => 'Tipo';

  @override
  String get desktopSortModified => 'Data di modifica';

  @override
  String get desktopViewOptions => 'Opzioni vista';

  @override
  String get desktopUntitledFolder => 'cartella senza titolo';

  @override
  String get desktopOpen => 'Apri';

  @override
  String get desktopMoveToTrash => 'Sposta nel Cestino';

  @override
  String get desktopRename => 'Rinomina';

  @override
  String get desktopDuplicate => 'Duplica';

  @override
  String get desktopQuickPreview => 'Anteprima rapida';

  @override
  String get desktopCustomizeFolder => 'Personalizza cartella…';

  @override
  String get desktopTags => 'Tag…';

  @override
  String get desktopSortSize => 'Dimensione';

  @override
  String get desktopCleanUp => 'Allinea';

  @override
  String get desktopCleanUpBy => 'Disponi per';

  @override
  String get ccNightLight => 'Luce notturna';

  @override
  String get chooseWidgets => 'Scegli i widget';

  @override
  String get clearAllNotifications => 'Elimina tutte le notifiche';

  @override
  String get replyPlaceholder => 'Rispondi';

  @override
  String get powerRestartTitle => 'Riavviare il computer adesso?';

  @override
  String powerRestartBody(int seconds) {
    return 'Silica si riavvierà automaticamente tra $seconds secondi.';
  }

  @override
  String get powerRestartButton => 'Riavvia';

  @override
  String get powerShutDownTitle => 'Spegnere il computer adesso?';

  @override
  String powerShutDownBody(int seconds) {
    return 'Silica si spegnerà automaticamente tra $seconds secondi.';
  }

  @override
  String get powerShutDownButton => 'Spegni';

  @override
  String powerLogOutTitle(String user) {
    return 'Disconnettere $user e chiudere tutte le app?';
  }

  @override
  String powerLogOutBody(int seconds) {
    return 'La disconnessione avverrà automaticamente tra $seconds secondi.';
  }

  @override
  String get powerLogOutButton => 'Esci';

  @override
  String get powerReopenWindows => 'Riapri le finestre al prossimo accesso';

  @override
  String get batteryMenuTitle => 'Batteria';

  @override
  String batteryMenuCharge(int percent) {
    return 'Carica al $percent %';
  }

  @override
  String get batteryMenuOnBattery => 'Alimentazione: batteria';

  @override
  String get batteryMenuPowerAdapter => 'Alimentazione: alimentatore';

  @override
  String get batteryMenuSettings => 'Impostazioni Batteria…';

  @override
  String get silicaHelp => 'Aiuto di Silica';

  @override
  String get ccNearbySharing => 'Invio rapido';

  @override
  String get ccNotAvailableYet => 'Non ancora disponibile';

  @override
  String get ccStageView => 'Vista scena';

  @override
  String get ccScreenMirroring => 'Duplicazione schermo';

  @override
  String get ccEditControls => 'Modifica controlli';

  @override
  String get ccDone => 'Fine';

  @override
  String get ccSearchControls => 'Cerca nei controlli';

  @override
  String get ccAllControls => 'Tutti i controlli';

  @override
  String get ccCategoryConnectivity => 'Connettività';

  @override
  String get ccCategoryDisplay => 'Schermo e luminosità';

  @override
  String get ccCategoryDesktop => 'Scrivania e finestre';

  @override
  String get ccNowPlaying => 'In riproduzione';

  @override
  String get ccGalleryHint =>
      'Trascina un controllo nel Centro di Controllo per aggiungerlo, oppure fai clic su di esso.';

  @override
  String get ccInCenter => 'Nel Centro di Controllo';

  @override
  String get ccSize => 'Dimensioni';

  @override
  String get ccSizeSmall => 'Piccolo';

  @override
  String get ccSizeMedium => 'Medio';

  @override
  String get ccSizeLarge => 'Grande';

  @override
  String get ccRemove => 'Rimuovi';

  @override
  String get ccCopyToMenuBar => 'Copia nella barra dei menu';

  @override
  String get ccRemoveFromMenuBar => 'Rimuovi dalla barra dei menu';

  @override
  String get ccWifiSettings => 'Impostazioni Wi-Fi…';

  @override
  String get ccBluetoothSettings => 'Impostazioni Bluetooth…';

  @override
  String get ccSoundSettings => 'Impostazioni Suono…';

  @override
  String get ccDisplaySettings => 'Impostazioni Schermi…';

  @override
  String get ccFocusSettings => 'Impostazioni Full immersion…';

  @override
  String get ccPrivacyWarning => 'Avviso sulla privacy';

  @override
  String ccPrivacyRecent(String app) {
    return '$app di recente';
  }

  @override
  String ccPrivacyNow(String app) {
    return '$app';
  }

  @override
  String get ccKeyboardBrightness => 'Luminosità tastiera';

  @override
  String get dockHidingOn => 'Nascondi automaticamente';

  @override
  String get dockHidingOff => 'Non nascondere più';

  @override
  String get dockPositionOnScreen => 'Posizione sullo schermo';

  @override
  String get dockPositionLeft => 'A sinistra';

  @override
  String get dockPositionBottom => 'In basso';

  @override
  String get dockPositionRight => 'A destra';

  @override
  String get dockMinimizeEffect => 'Effetto di riduzione';

  @override
  String get dockEffectGenie => 'Effetto genio';

  @override
  String get dockEffectScale => 'Effetto scala';

  @override
  String get dockSettingsEllipsis => 'Impostazioni del Dock…';

  @override
  String get dockShowApps => 'Mostra le app';

  @override
  String get dockNewSmartFolder => 'Nuova cartella smart';

  @override
  String get dockFind => 'Trova…';

  @override
  String get dockGoToFolder => 'Vai alla cartella…';

  @override
  String get dockConnectToServer => 'Connetti al server…';

  @override
  String get dockHide => 'Nascondi';

  @override
  String get placeApplications => 'Applicazioni';

  @override
  String get placeDocuments => 'Documenti';

  @override
  String get placeDownloads => 'Download';

  @override
  String get placeRecents => 'Recenti';

  @override
  String get placeDesktop => 'Scrivania';

  @override
  String get dockHideOthers => 'Nascondi le altre';

  @override
  String get dockForceQuit => 'Forza l\'uscita';

  @override
  String get dockOpenAtLogin => 'Apri all\'accesso';

  @override
  String get dockShowInFileManager => 'Mostra in Mustache';

  @override
  String get dockAssignTo => 'Assegna alla scrivania';

  @override
  String get faroScopeApps => 'Applicazioni';

  @override
  String get faroScopeFiles => 'File';

  @override
  String get faroScopeActions => 'Azioni';

  @override
  String get faroScopeClipboard => 'Appunti';

  @override
  String get faroSectionSystem => 'Sistema';

  @override
  String get faroSearchFiles => 'Cerca nei file';

  @override
  String get faroSearchActions => 'Cerca nelle azioni';

  @override
  String get faroSearchClipboard => 'Cerca negli appunti';

  @override
  String get faroClearHistory => 'Cancella cronologia';

  @override
  String get faroImage => 'Immagine';

  @override
  String get faroSystemAction => 'Sistema';

  @override
  String faroCopied(String time) {
    return 'Copiato $time';
  }

  @override
  String get faroActionScreenshotScreen => 'Istantanea dello schermo';

  @override
  String get faroActionScreenshotSelection => 'Istantanea di una parte';

  @override
  String get faroActionScreenshotToolbar => 'Strumenti istantanea';

  @override
  String get faroActionSystemDarkToggle => 'Cambia aspetto chiaro/scuro';

  @override
  String get faroActionSystemMuteToggle => 'Attiva/disattiva audio';

  @override
  String get faroActionSystemVolumeUp => 'Alza il volume';

  @override
  String get faroActionSystemVolumeDown => 'Abbassa il volume';

  @override
  String get faroActionSystemBrightnessUp => 'Aumenta la luminosità';

  @override
  String get faroActionSystemBrightnessDown => 'Diminuisci la luminosità';

  @override
  String get faroActionPanoramaToggle => 'Panorama';

  @override
  String get faroActionPanoramaApp => 'Finestre dell\'app';

  @override
  String get faroActionDesktopShow => 'Mostra la scrivania';

  @override
  String get faroActionSpaceAdd => 'Nuova scrivania';

  @override
  String get faroActionSpaceNext => 'Scrivania successiva';

  @override
  String get faroActionSpacePrevious => 'Scrivania precedente';

  @override
  String get faroActionLauncherToggle => 'App';

  @override
  String get faroActionNotificationsToggle => 'Centro Notifiche';

  @override
  String get faroActionControlcenterToggle => 'Centro di Controllo';

  @override
  String get faroActionWindowMinimize => 'Riduci la finestra';

  @override
  String get faroActionWindowFullscreen => 'Schermo intero';

  @override
  String get faroActionWindowTileLeft => 'Metà sinistra';

  @override
  String get faroActionWindowTileRight => 'Metà destra';

  @override
  String get faroActionWindowTileTop => 'Metà superiore';

  @override
  String get faroActionWindowTileBottom => 'Metà inferiore';

  @override
  String get faroActionWindowTileFill => 'Riempi';

  @override
  String get faroActionWindowArrange2 => 'Disponi due finestre';

  @override
  String get faroActionWindowArrange3 => 'Disponi tre finestre';

  @override
  String get faroActionWindowArrange4 => 'Disponi quattro finestre';

  @override
  String get faroActionAppForceQuitPanel => 'Uscita forzata';

  @override
  String get faroActionSessionQuit => 'Esci dalla sessione';

  @override
  String get authTitle =>
      'Una modifica al sistema richiede il permesso di un amministratore.';

  @override
  String authTitleApp(String app) {
    return '$app richiede il permesso di un amministratore.';
  }

  @override
  String get authPrompt => 'Inserisci la password per consentirlo.';

  @override
  String get authUnavailable =>
      'Non è stato possibile verificare la password: riprova.';

  @override
  String get authUserName => 'Nome utente';

  @override
  String get authPassword => 'Password';

  @override
  String get authAllow => 'Consenti';

  @override
  String get menubarTags => 'Tag';

  @override
  String get menubarTagRed => 'Rosso';

  @override
  String get menubarTagOrange => 'Arancione';

  @override
  String get menubarTagYellow => 'Giallo';

  @override
  String get menubarTagGreen => 'Verde';

  @override
  String get menubarTagBlue => 'Blu';

  @override
  String get menubarTagPurple => 'Viola';

  @override
  String get menubarTagGray => 'Grigio';

  @override
  String get ccBuiltInDisplay => 'Schermo integrato';

  @override
  String get ccNearby => 'Vicino';

  @override
  String get ccDevices => 'Dispositivi';

  @override
  String get notificationsToday => 'Oggi';

  @override
  String get notificationsEarlier => 'Prima';

  @override
  String get notificationsClearAll => 'Cancella tutto';

  @override
  String get notificationsNow => 'ora';

  @override
  String get lockPassword => 'Inserisci la password';

  @override
  String get lockIncorrect => 'Password errata.';

  @override
  String get lockUnavailable => 'Non è stato possibile verificare la password.';

  @override
  String get lockPasswordField => 'Password, campo protetto';

  @override
  String get lockHelpButton => 'Mostra il suggerimento della password';

  @override
  String get lockSubmit => 'Sblocca';

  @override
  String get lockNoHint =>
      'Per questo account non è stato impostato un suggerimento per la password.';

  @override
  String get lockSleep => 'Standby';

  @override
  String get lockRestart => 'Riavvia';

  @override
  String get lockShutDown => 'Spegni';

  @override
  String lockNotificationCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count notifiche',
      one: '1 notifica',
    );
    return '$_temp0';
  }

  @override
  String get statusWifi => 'Wi-Fi';

  @override
  String get statusWifiOff => 'disattivato';

  @override
  String get statusWifiNotConnected => 'non connesso';

  @override
  String statusWifiConnected(String network) {
    return 'connesso a $network';
  }

  @override
  String get statusEthernet => 'Ethernet';

  @override
  String get statusBattery => 'Batteria';

  @override
  String statusBatteryLevel(int percent) {
    return '$percent percento';
  }

  @override
  String statusBatteryCharging(int percent) {
    return '$percent percento, in carica';
  }

  @override
  String get statusInputSource => 'Sorgente di input';

  @override
  String get statusInputSettings => 'Impostazioni Tastiera…';

  @override
  String get statusBluetooth => 'Bluetooth';

  @override
  String get statusSound => 'Suono';

  @override
  String get statusSearch => 'Cerca';

  @override
  String get statusClock => 'Orologio';

  @override
  String get statusControlCenter => 'Centro di Controllo';

  @override
  String get statusFocus => 'Full immersion';

  @override
  String get statusNowPlaying => 'In riproduzione';

  @override
  String gkConfirmTitle(String app) {
    return '“$app” è un’app scaricata da Internet. Vuoi davvero aprirla?';
  }

  @override
  String gkFromImage(String image) {
    return 'Proviene dall’immagine disco “$image”.';
  }

  @override
  String gkFromSite(String site) {
    return 'È stata scaricata da $site.';
  }

  @override
  String gkFromApp(String agent) {
    return 'È stata salvata da $agent.';
  }

  @override
  String gkDeveloperVerified(String developer) {
    return 'Sviluppatore: $developer (identificato).';
  }

  @override
  String gkDeveloperSelfSigned(String developer) {
    return 'Sviluppatore: $developer (non identificato).';
  }

  @override
  String gkBlockTitle(String app) {
    return '“$app” non è stata aperta';
  }

  @override
  String gkBlockUnsigned(String app) {
    return 'Silica non può verificare che “$app” sia priva di malware: non è firmata dal suo sviluppatore.';
  }

  @override
  String gkBlockSelfSigned(String app) {
    return 'Silica non può verificare che “$app” sia priva di malware: il suo sviluppatore non è identificato.';
  }

  @override
  String gkBlockTampered(String app) {
    return '“$app” è stata modificata dopo la firma dello sviluppatore, oppure è danneggiata.';
  }

  @override
  String gkBlockInvalid(String app) {
    return '“$app” non è un’app valida.';
  }

  @override
  String get gkBlockHint =>
      'Se ti fidi, puoi aprirla da Privacy e sicurezza nelle Impostazioni.';

  @override
  String gkMalwareTitle(String app) {
    return '“$app” danneggerà il computer.';
  }

  @override
  String get gkMalwareBody =>
      'La chiave del suo sviluppatore è stata revocata. Ti conviene spostarla nel Cestino.';

  @override
  String get gkOpen => 'Apri';

  @override
  String get gkDone => 'Fine';

  @override
  String get gkMoveToTrash => 'Sposta nel Cestino';
}
