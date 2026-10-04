//! HBC's system-language selection and UI string catalog.
use crate::metadata::ThemeMetadata;
use alloc::{format, string::String};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Language {
    #[default]
    English,
    Spanish,
    French,
    German,
    Dutch,
    Italian,
    Japanese,
}

impl Language {
    /// Select the C HBC catalog for a Wii language value. Japanese needs a theme
    /// with explicit `ja` support, as in the original channel.
    pub fn from_console(value: i32, theme: Option<&ThemeMetadata>) -> Self {
        match value {
            2 => Self::German,
            3 => Self::French,
            4 => Self::Spanish,
            5 => Self::Italian,
            6 => Self::Dutch,
            0 if theme.is_some_and(|theme| theme.languages.iter().any(|lang| lang == "ja")) => {
                Self::Japanese
            }
            _ => Self::English,
        }
    }

    pub fn text<'a>(self, source: &'a str) -> &'a str {
        use Language::*;
        match (self, source) {
            (Spanish, "Back") => "Atrás",
            (French, "Back") => "Retour",
            (German, "Back") => "Zurück",
            (Dutch, "Back") => "Terug",
            (Italian, "Back") => "Indietro",
            (Japanese, "Back") => "戻る",
            (Spanish, "About") => "Acerca de",
            (French, "About") => "À propos de",
            (German, "About") => "Info",
            (Dutch, "About") => "Info",
            (Italian, "About") => "Informazioni",
            (Japanese, "About") => "このアプリケーションについて",
            (Spanish, "Confirmation") => "Confirmación",
            (French, "Confirmation") => "Confirmation",
            (German, "Confirmation") => "Bestätigung",
            (Dutch, "Confirmation") => "Bevestiging",
            (Italian, "Confirmation") => "Conferma",
            (Japanese, "Confirmation") => "確認",
            (Spanish, "Launch BootMii") => "Lanzar BootMii",
            (French, "Launch BootMii") => "Lancer BootMii",
            (German, "Launch BootMii") => "BootMii starten",
            (Dutch, "Launch BootMii") => "BootMii starten",
            (Italian, "Launch BootMii") => "Lanciare BootMii",
            (Japanese, "Launch BootMii") => "BootMiiを起動",
            (Spanish, "Exit to System Menu") => "Salir al menú del sistema",
            (French, "Exit to System Menu") => "Retour au menu système",
            (German, "Exit to System Menu") => "Beenden, zum Systemmenü",
            (Dutch, "Exit to System Menu") => "Terug naar System Menu",
            (Italian, "Exit to System Menu") => "Ritorna al Menu di Sistema",
            (Japanese, "Exit to System Menu") => "システムメニューへ戻る",
            (Spanish, "Shutdown") => "Apagar",
            (French, "Shutdown") => "Arrêt",
            (German, "Shutdown") => "Herunterfahren",
            (Dutch, "Shutdown") => "Afsluiten",
            (Italian, "Shutdown") => "Spegni",
            (Japanese, "Shutdown") => "シャットダウン",
            (Spanish, "Options") => "Opciones",
            (French, "Options") => "Options",
            (German, "Options") => "Einstellungen",
            (Dutch, "Options") => "Opties",
            (Italian, "Options") => "Opzioni",
            (Japanese, "Options") => "オプション",
            (Spanish, "Delete") => "Eliminar",
            (French, "Delete") => "Supprimer",
            (German, "Delete") => "Löschen",
            (Dutch, "Delete") => "Verwijder",
            (Italian, "Delete") => "Elimina",
            (Japanese, "Delete") => "削除",
            (Spanish, "Load") => "Cargar",
            (French, "Load") => "Charger",
            (German, "Load") => "Laden",
            (Dutch, "Load") => "Start",
            (Italian, "Load") => "Carica",
            (Japanese, "Load") => "起動",
            (Spanish, "Name") => "Nombre",
            (French, "Name") => "Nom",
            (German, "Name") => "Name",
            (Dutch, "Name") => "Naam",
            (Italian, "Name") => "Nome",
            (Japanese, "Name") => "名前",
            (Spanish, "Date") => "Fecha",
            (French, "Date") => "Date",
            (German, "Date") => "Datum",
            (Dutch, "Date") => "Datum",
            (Italian, "Date") => "Data",
            (Japanese, "Date") => "日付",
            (Spanish, "Yes") => "Sí",
            (French, "Yes") => "Oui",
            (German, "Yes") => "Ja",
            (Dutch, "Yes") => "Ja",
            (Italian, "Yes") => "Sì",
            (Japanese, "Yes") => "はい",
            (Spanish, "No") => "No",
            (French, "No") => "Non",
            (German, "No") => "Nein",
            (Dutch, "No") => "Nee",
            (Italian, "No") => "No",
            (Japanese, "No") => "いいえ",
            (Spanish, "Apply") => "Aplicar",
            (French, "Apply") => "Appliquer",
            (German, "Apply") => "Übernehmen",
            (Dutch, "Apply") => "Toepassen",
            (Italian, "Apply") => "Applica",
            (Japanese, "Apply") => "適用",
            (Spanish, "Device:") => "Dispositivo:",
            (French, "Device:") => "Dispositif:",
            (German, "Device:") => "Quelle:",
            (Dutch, "Device:") => "Apparaat:",
            (Italian, "Device:") => "Periferica:",
            (Japanese, "Device:") => "デバイス:",
            (Spanish, "Sort applications by:") => "Ordenar aplicaciones por:",
            (French, "Sort applications by:") => "Trier les applications par:",
            (German, "Sort applications by:") => "Anwendungen sortieren nach:",
            (Dutch, "Sort applications by:") => "Sorteer applicaties op:",
            (Italian, "Sort applications by:") => "Ordina applicazioni per:",
            (Japanese, "Sort applications by:") => "アプリケーションの並べ替え順序",
            (Spanish, "Internal SD Slot") => "Ranura SD Interna",
            (French, "Internal SD Slot") => "Lecteur de carte SD",
            (German, "Internal SD Slot") => "Interner SD Slot",
            (Dutch, "Internal SD Slot") => "Intern SD Slot",
            (Italian, "Internal SD Slot") => "Slot SD Interno",
            (Japanese, "Internal SD Slot") => "内部SDスロット",
            (Spanish, "USB device") => "Dispositivo USB",
            (French, "USB device") => "Dispositif USB",
            (German, "USB device") => "USB-Laufwerk",
            (Dutch, "USB device") => "USB apparaat",
            (Italian, "USB device") => "Pendrive USB",
            (Japanese, "USB device") => "USBデバイス",
            (Spanish, "Delete this application?") => {
                "¿Estás seguro de que quieres eliminar esta aplicación?"
            }
            (French, "Delete this application?") => {
                "Voulez-vous vraiment supprimer cette application?"
            }
            (German, "Delete this application?") => "Soll die Anwendung wirklich gelöscht werden?",
            (Dutch, "Delete this application?") => "Wil je echt deze applicatie verwijderen?",
            (Italian, "Delete this application?") => {
                "Sei sicuro di voler cancellare questa applicazione?"
            }
            (Japanese, "Delete this application?") => "本当にこのアプリケーションを削除しますか。",
            (Spanish, "No description available.") => "descripción no disponible",
            (French, "No description available.") => "aucune description disponible",
            (German, "No description available.") => "keine Beschreibung",
            (Dutch, "No description available.") => "Geen omschrijving beschikbaar",
            (Italian, "No description available.") => "nessuna descrizione disponibile",
            (Japanese, "No description available.") => "詳細情報はありません",
            (Spanish, "Information") => "Información",
            (French, "Information") => "Information",
            (German, "Information") => "Information",
            (Dutch, "Information") => "Informatie",
            (Italian, "Information") => "Informazioni",
            (Japanese, "Information") => "報告",
            (Spanish, "Warning") => "Advertencia",
            (French, "Warning") => "Avertissement",
            (German, "Warning") => "Warnung",
            (Dutch, "Warning") => "Waarschuwing",
            (Italian, "Warning") => "Avviso",
            (Japanese, "Warning") => "警告",
            (Spanish, "Error") => "Error",
            (French, "Error") => "Erreur",
            (German, "Error") => "Fehler",
            (Dutch, "Error") => "Fout",
            (Italian, "Error") => "Errore",
            (Japanese, "Error") => "エラー",
            (Spanish, "Theme:") => "Tema:",
            (French, "Theme:") => "Thème :",
            (German, "Theme:") => "Thema:",
            (Dutch, "Theme:") => "Thema:",
            (Italian, "Theme:") => "Tema:",
            (Japanese, "Theme:") => "テーマ:",
            (Spanish, "Notice") => "Aviso",
            (French, "Notice") => "Avis",
            (German, "Notice") => "Hinweis",
            (Dutch, "Notice") => "Melding",
            (Italian, "Notice") => "Avviso",
            (Japanese, "Notice") => "お知らせ",
            (Spanish, "OK") => "Aceptar",
            (French, "OK") => "Ok",
            (German, "OK") => "OK",
            (Dutch, "OK") => "OK",
            (Italian, "OK") => "Ok",
            (Japanese, "OK") => "Ok",
            (Spanish, "Press B to return") => "Pulsa B para volver",
            (French, "Press B to return") => "Appuyez sur B pour revenir",
            (German, "Press B to return") => "Zum Zurückkehren B drücken",
            (Dutch, "Press B to return") => "Druk op B om terug te gaan",
            (Italian, "Press B to return") => "Premi B per tornare indietro",
            (Japanese, "Press B to return") => "Bボタンで戻る",
            (Spanish, "Launch applications from removable storage.") => {
                "Inicia aplicaciones desde almacenamiento extraíble."
            }
            (French, "Launch applications from removable storage.") => {
                "Lancez des applications depuis un support amovible."
            }
            (German, "Launch applications from removable storage.") => {
                "Starte Anwendungen von Wechseldatenträgern."
            }
            (Dutch, "Launch applications from removable storage.") => {
                "Start applicaties vanaf verwijderbare opslag."
            }
            (Italian, "Launch applications from removable storage.") => {
                "Avvia applicazioni da supporti rimovibili."
            }
            (Japanese, "Launch applications from removable storage.") => {
                "リムーバブルストレージからアプリを起動します。"
            }
            (Spanish, "Native Rust port under development.") => {
                "Puerto nativo de Rust en desarrollo."
            }
            (French, "Native Rust port under development.") => {
                "Port Rust natif en cours de développement."
            }
            (German, "Native Rust port under development.") => "Nativer Rust-Port in Entwicklung.",
            (Dutch, "Native Rust port under development.") => "Native Rust-port in ontwikkeling.",
            (Italian, "Native Rust port under development.") => "Port nativo Rust in sviluppo.",
            (Japanese, "Native Rust port under development.") => "Rustへの移植版を開発中です。",
            (Spanish, "Available") => "Disponible",
            (French, "Available") => "Disponible",
            (German, "Available") => "Verfügbar",
            (Dutch, "Available") => "Beschikbaar",
            (Italian, "Available") => "Disponibile",
            (Japanese, "Available") => "利用可能",
            (Spanish, "Unavailable") => "No disponible",
            (French, "Unavailable") => "Indisponible",
            (German, "Unavailable") => "Nicht verfügbar",
            (Dutch, "Unavailable") => "Niet beschikbaar",
            (Italian, "Unavailable") => "Non disponibile",
            (Japanese, "Unavailable") => "利用不可",
            (Spanish, "Checking") => "Comprobando",
            (French, "Checking") => "Vérification",
            (German, "Checking") => "Prüfung",
            (Dutch, "Checking") => "Controleren",
            (Italian, "Checking") => "Verifica",
            (Japanese, "Checking") => "確認中",
            (Spanish, "SD Gecko A") => "SDGecko en Slot A",
            (French, "SD Gecko A") => "Lecteur SDGecko A",
            (German, "SD Gecko A") => "SDGecko Slot A",
            (Dutch, "SD Gecko A") => "SDGecko Slot A",
            (Italian, "SD Gecko A") => "SDGecko Slot A",
            (Japanese, "SD Gecko A") => "SDGeckoのスロットA",
            (Spanish, "SD Gecko B") => "SDGecko en Slot B",
            (French, "SD Gecko B") => "Lecteur SDGecko B",
            (German, "SD Gecko B") => "SDGecko Slot B",
            (Dutch, "SD Gecko B") => "SDGecko Slot B",
            (Italian, "SD Gecko B") => "SDGecko Slot B",
            (Japanese, "SD Gecko B") => "SDGeckoのスロットB",
            (Spanish, "Install received ZIP? Existing files may be replaced.") => {
                "¿Instalar ZIP recibido? Los archivos existentes pueden reemplazarse."
            }
            (French, "Install received ZIP? Existing files may be replaced.") => {
                "Installer le ZIP reçu ? Les fichiers existants peuvent être remplacés."
            }
            (German, "Install received ZIP? Existing files may be replaced.") => {
                "Empfangene ZIP installieren? Vorhandene Dateien können überschrieben werden."
            }
            (Dutch, "Install received ZIP? Existing files may be replaced.") => {
                "Ontvangen ZIP installeren? Bestaande bestanden kunnen worden vervangen."
            }
            (Italian, "Install received ZIP? Existing files may be replaced.") => {
                "Installare lo ZIP ricevuto? I file esistenti potrebbero essere sostituiti."
            }
            (Japanese, "Install received ZIP? Existing files may be replaced.") => {
                "受信したZIPをインストールしますか。既存ファイルは置き換えられる場合があります。"
            }
            (Spanish, "Receiving upload") => "Recibiendo datos",
            (French, "Receiving upload") => "Réception des données",
            (German, "Receiving upload") => "Upload wird empfangen",
            (Dutch, "Receiving upload") => "Upload ontvangen",
            (Italian, "Receiving upload") => "Ricezione in corso",
            (Japanese, "Receiving upload") => "受信中",
            (Spanish, "Installing ZIP") => "Instalando ZIP",
            (French, "Installing ZIP") => "Installation du ZIP",
            (German, "Installing ZIP") => "ZIP wird installiert",
            (Dutch, "Installing ZIP") => "ZIP installeren",
            (Italian, "Installing ZIP") => "Installazione ZIP",
            (Japanese, "Installing ZIP") => "ZIPをインストール中",
            (Spanish, "Please wait...") => "Espera...",
            (French, "Please wait...") => "Veuillez patienter...",
            (German, "Please wait...") => "Bitte warten...",
            (Dutch, "Please wait...") => "Even wachten...",
            (Italian, "Please wait...") => "Attendere...",
            (Japanese, "Please wait...") => "お待ちください...",
            (Spanish, "Scanning storage...") => "Buscando almacenamiento...",
            (French, "Scanning storage...") => "Analyse du stockage...",
            (German, "Scanning storage...") => "Speicher wird durchsucht...",
            (Dutch, "Scanning storage...") => "Opslag scannen...",
            (Italian, "Scanning storage...") => "Scansione della memoria...",
            (Japanese, "Scanning storage...") => "ストレージを検索中...",
            (Spanish, "No SD or USB storage device is available.") => {
                "No hay ningún dispositivo SD o USB disponible."
            }
            (French, "No SD or USB storage device is available.") => {
                "Aucun périphérique SD ou USB disponible."
            }
            (German, "No SD or USB storage device is available.") => {
                "Kein SD- oder USB-Speichergerät verfügbar."
            }
            (Dutch, "No SD or USB storage device is available.") => {
                "Geen SD- of USB-opslagapparaat beschikbaar."
            }
            (Italian, "No SD or USB storage device is available.") => {
                "Nessun dispositivo di archiviazione SD o USB disponibile."
            }
            (Japanese, "No SD or USB storage device is available.") => {
                "SDまたはUSBストレージがありません。"
            }
            (Spanish, "The selected storage device is not available.") => {
                "El dispositivo de almacenamiento seleccionado no está disponible."
            }
            (French, "The selected storage device is not available.") => {
                "Le périphérique de stockage sélectionné est indisponible."
            }
            (German, "The selected storage device is not available.") => {
                "Das ausgewählte Speichergerät ist nicht verfügbar."
            }
            (Dutch, "The selected storage device is not available.") => {
                "Het geselecteerde opslagapparaat is niet beschikbaar."
            }
            (Italian, "The selected storage device is not available.") => {
                "Il dispositivo di archiviazione selezionato non è disponibile."
            }
            (Japanese, "The selected storage device is not available.") => {
                "選択したストレージは利用できません。"
            }
            (Spanish, "Storage was removed. Connect an SD or USB device.") => {
                "Se ha retirado el almacenamiento. Conecta un dispositivo SD o USB."
            }
            (French, "Storage was removed. Connect an SD or USB device.") => {
                "Le stockage a été retiré. Connectez un périphérique SD ou USB."
            }
            (German, "Storage was removed. Connect an SD or USB device.") => {
                "Speicher wurde entfernt. SD- oder USB-Gerät anschließen."
            }
            (Dutch, "Storage was removed. Connect an SD or USB device.") => {
                "Opslag is verwijderd. Sluit een SD- of USB-apparaat aan."
            }
            (Italian, "Storage was removed. Connect an SD or USB device.") => {
                "Il dispositivo è stato rimosso. Collega un dispositivo SD o USB."
            }
            (Japanese, "Storage was removed. Connect an SD or USB device.") => {
                "ストレージが取り外されました。SDまたはUSBを接続してください。"
            }
            (Spanish, "Could not read the apps folder on this device.") => {
                "No se pudo leer la carpeta de aplicaciones de este dispositivo."
            }
            (French, "Could not read the apps folder on this device.") => {
                "Impossible de lire le dossier des applications sur ce périphérique."
            }
            (German, "Could not read the apps folder on this device.") => {
                "Der Apps-Ordner auf diesem Gerät konnte nicht gelesen werden."
            }
            (Dutch, "Could not read the apps folder on this device.") => {
                "De apps-map op dit apparaat kan niet worden gelezen."
            }
            (Italian, "Could not read the apps folder on this device.") => {
                "Impossibile leggere la cartella delle applicazioni su questo dispositivo."
            }
            (Japanese, "Could not read the apps folder on this device.") => {
                "このデバイスのappsフォルダーを読み込めません。"
            }
            (Spanish, "Could not start storage discovery.") => {
                "No se pudo iniciar la búsqueda de almacenamiento."
            }
            (French, "Could not start storage discovery.") => {
                "Impossible de démarrer la recherche du stockage."
            }
            (German, "Could not start storage discovery.") => {
                "Die Speichererkennung konnte nicht gestartet werden."
            }
            (Dutch, "Could not start storage discovery.") => {
                "Opslagdetectie kan niet worden gestart."
            }
            (Italian, "Could not start storage discovery.") => {
                "Impossibile avviare la ricerca dei dispositivi."
            }
            (Japanese, "Could not start storage discovery.") => {
                "ストレージの検索を開始できません。"
            }
            (Spanish, "Could not start the network receiver.") => {
                "No se pudo iniciar el receptor de red."
            }
            (French, "Could not start the network receiver.") => {
                "Impossible de démarrer le récepteur réseau."
            }
            (German, "Could not start the network receiver.") => {
                "Der Netzwerkempfänger konnte nicht gestartet werden."
            }
            (Dutch, "Could not start the network receiver.") => {
                "De netwerkontvanger kan niet worden gestart."
            }
            (Italian, "Could not start the network receiver.") => {
                "Impossibile avviare il ricevitore di rete."
            }
            (Japanese, "Could not start the network receiver.") => {
                "ネットワーク受信機を起動できません。"
            }
            (Spanish, "Could not refresh storage devices.") => {
                "No se pudieron actualizar los dispositivos de almacenamiento."
            }
            (French, "Could not refresh storage devices.") => {
                "Impossible d’actualiser les périphériques de stockage."
            }
            (German, "Could not refresh storage devices.") => {
                "Die Speichergeräte konnten nicht aktualisiert werden."
            }
            (Dutch, "Could not refresh storage devices.") => {
                "Opslagapparaten kunnen niet worden vernieuwd."
            }
            (Italian, "Could not refresh storage devices.") => {
                "Impossibile aggiornare i dispositivi di archiviazione."
            }
            (Japanese, "Could not refresh storage devices.") => {
                "ストレージデバイスを更新できません。"
            }
            (Spanish, "Could not install the app or apply the theme.") => {
                "No se pudo instalar la aplicación ni aplicar el tema."
            }
            (French, "Could not install the app or apply the theme.") => {
                "Impossible d’installer l’application ou d’appliquer le thème."
            }
            (German, "Could not install the app or apply the theme.") => {
                "Die App konnte nicht installiert oder das Thema nicht angewendet werden."
            }
            (Dutch, "Could not install the app or apply the theme.") => {
                "De app kan niet worden geïnstalleerd of het thema kan niet worden toegepast."
            }
            (Italian, "Could not install the app or apply the theme.") => {
                "Impossibile installare l’app o applicare il tema."
            }
            (Japanese, "Could not install the app or apply the theme.") => {
                "アプリをインストールするかテーマを適用できません。"
            }
            (Spanish, "Could not save this theme to NAND.") => {
                "No se pudo guardar este tema en NAND."
            }
            (French, "Could not save this theme to NAND.") => {
                "Impossible d’enregistrer ce thème dans la NAND."
            }
            (German, "Could not save this theme to NAND.") => {
                "Dieses Thema konnte nicht im NAND gespeichert werden."
            }
            (Dutch, "Could not save this theme to NAND.") => {
                "Dit thema kan niet op NAND worden opgeslagen."
            }
            (Italian, "Could not save this theme to NAND.") => {
                "Impossibile salvare questo tema nella NAND."
            }
            (Japanese, "Could not save this theme to NAND.") => {
                "このテーマをNANDに保存できません。"
            }
            (Spanish, "Could not read or validate this theme.") => {
                "No se pudo leer o validar este tema."
            }
            (French, "Could not read or validate this theme.") => {
                "Impossible de lire ou valider ce thème."
            }
            (German, "Could not read or validate this theme.") => {
                "Dieses Thema konnte nicht gelesen oder geprüft werden."
            }
            (Dutch, "Could not read or validate this theme.") => {
                "Dit thema kan niet worden gelezen of gecontroleerd."
            }
            (Italian, "Could not read or validate this theme.") => {
                "Impossibile leggere o convalidare questo tema."
            }
            (Japanese, "Could not read or validate this theme.") => {
                "このテーマを読み込むか検証できません。"
            }
            (Spanish, "Could not read or validate this application.") => {
                "No se pudo leer o validar esta aplicación."
            }
            (French, "Could not read or validate this application.") => {
                "Impossible de lire ou valider cette application."
            }
            (German, "Could not read or validate this application.") => {
                "Diese Anwendung konnte nicht gelesen oder geprüft werden."
            }
            (Dutch, "Could not read or validate this application.") => {
                "Deze applicatie kan niet worden gelezen of gecontroleerd."
            }
            (Italian, "Could not read or validate this application.") => {
                "Impossibile leggere o convalidare questa applicazione."
            }
            (Japanese, "Could not read or validate this application.") => {
                "このアプリを読み込むか検証できません。"
            }
            (Spanish, "Could not delete this application.") => {
                "No se pudo eliminar esta aplicación."
            }
            (French, "Could not delete this application.") => {
                "Impossible de supprimer cette application."
            }
            (German, "Could not delete this application.") => {
                "Diese Anwendung konnte nicht gelöscht werden."
            }
            (Dutch, "Could not delete this application.") => {
                "Deze applicatie kan niet worden verwijderd."
            }
            (Italian, "Could not delete this application.") => {
                "Impossibile eliminare questa applicazione."
            }
            (Japanese, "Could not delete this application.") => "このアプリを削除できません。",
            (Spanish, "Could not stop storage. BootMii launch stopped.") => {
                "No se pudo detener el almacenamiento. Se canceló el inicio de BootMii."
            }
            (French, "Could not stop storage. BootMii launch stopped.") => {
                "Impossible d’arrêter le stockage. Le lancement de BootMii est annulé."
            }
            (German, "Could not stop storage. BootMii launch stopped.") => {
                "Der Speicher konnte nicht gestoppt werden. BootMii wird nicht gestartet."
            }
            (Dutch, "Could not stop storage. BootMii launch stopped.") => {
                "Opslag kan niet worden gestopt. BootMii wordt niet gestart."
            }
            (Italian, "Could not stop storage. BootMii launch stopped.") => {
                "Impossibile arrestare il dispositivo. Avvio di BootMii annullato."
            }
            (Japanese, "Could not stop storage. BootMii launch stopped.") => {
                "ストレージを停止できません。BootMiiの起動を中止しました。"
            }
            (Spanish, "Received data is not a valid Wii application.") => {
                "Los datos recibidos no son una aplicación válida de Wii."
            }
            (French, "Received data is not a valid Wii application.") => {
                "Les données reçues ne constituent pas une application Wii valide."
            }
            (German, "Received data is not a valid Wii application.") => {
                "Die empfangenen Daten sind keine gültige Wii-Anwendung."
            }
            (Dutch, "Received data is not a valid Wii application.") => {
                "De ontvangen gegevens zijn geen geldige Wii-applicatie."
            }
            (Italian, "Received data is not a valid Wii application.") => {
                "I dati ricevuti non sono un’applicazione Wii valida."
            }
            (Japanese, "Received data is not a valid Wii application.") => {
                "受信データは有効なWiiアプリではありません。"
            }
            (Spanish, "Received ZIP is not a valid app or theme.") => {
                "El ZIP recibido no contiene una aplicación ni un tema válidos."
            }
            (French, "Received ZIP is not a valid app or theme.") => {
                "Le ZIP reçu ne contient ni application ni thème valide."
            }
            (German, "Received ZIP is not a valid app or theme.") => {
                "Die empfangene ZIP-Datei enthält keine gültige App oder kein gültiges Thema."
            }
            (Dutch, "Received ZIP is not a valid app or theme.") => {
                "Het ontvangen ZIP-bestand bevat geen geldige app of thema."
            }
            (Italian, "Received ZIP is not a valid app or theme.") => {
                "Lo ZIP ricevuto non contiene un’app o un tema valido."
            }
            (Japanese, "Received ZIP is not a valid app or theme.") => {
                "受信したZIPに有効なアプリまたはテーマがありません。"
            }
            (Spanish, "Received ZIP is complete.") => "El ZIP recibido se instaló correctamente.",
            (French, "Received ZIP is complete.") => "Le ZIP reçu a été installé.",
            (German, "Received ZIP is complete.") => "Die empfangene ZIP-Datei wurde installiert.",
            (Dutch, "Received ZIP is complete.") => "Het ontvangen ZIP-bestand is geïnstalleerd.",
            (Italian, "Received ZIP is complete.") => "Lo ZIP ricevuto è stato installato.",
            (Japanese, "Received ZIP is complete.") => "受信したZIPをインストールしました。",
            (Spanish, "Could not install the received ZIP.") => "No se pudo instalar el ZIP recibido.",
            (French, "Could not install the received ZIP.") => "Impossible d’installer le ZIP reçu.",
            (German, "Could not install the received ZIP.") => "Die empfangene ZIP-Datei konnte nicht installiert werden.",
            (Dutch, "Could not install the received ZIP.") => "Het ontvangen ZIP-bestand kan niet worden geïnstalleerd.",
            (Italian, "Could not install the received ZIP.") => "Impossibile installare lo ZIP ricevuto.",
            (Japanese, "Could not install the received ZIP.") => "受信したZIPをインストールできません。",
            (Spanish, "Could not prepare the received application.") => "No se pudo preparar la aplicación recibida.",
            (French, "Could not prepare the received application.") => "Impossible de préparer l’application reçue.",
            (German, "Could not prepare the received application.") => "Die empfangene Anwendung konnte nicht vorbereitet werden.",
            (Dutch, "Could not prepare the received application.") => "De ontvangen applicatie kan niet worden voorbereid.",
            (Italian, "Could not prepare the received application.") => "Impossibile preparare l’applicazione ricevuta.",
            (Japanese, "Could not prepare the received application.") => "受信したアプリを準備できません。",
            (Spanish, "Could not prepare this application to run.") => "No se pudo preparar esta aplicación para ejecutarla.",
            (French, "Could not prepare this application to run.") => "Impossible de préparer cette application au lancement.",
            (German, "Could not prepare this application to run.") => "Diese Anwendung konnte nicht zum Start vorbereitet werden.",
            (Dutch, "Could not prepare this application to run.") => "Deze applicatie kan niet worden voorbereid om te starten.",
            (Italian, "Could not prepare this application to run.") => "Impossibile preparare questa applicazione per l’avvio.",
            (Japanese, "Could not prepare this application to run.") => "このアプリを起動する準備ができません。",
            (Spanish, "Could not change the storage device.") => "No se pudo cambiar el dispositivo de almacenamiento.",
            (French, "Could not change the storage device.") => "Impossible de changer de périphérique de stockage.",
            (German, "Could not change the storage device.") => "Das Speichergerät konnte nicht gewechselt werden.",
            (Dutch, "Could not change the storage device.") => "Het opslagapparaat kan niet worden gewijzigd.",
            (Italian, "Could not change the storage device.") => "Impossibile cambiare il dispositivo di archiviazione.",
            (Japanese, "Could not change the storage device.") => "ストレージデバイスを変更できません。",
            (Spanish, "Could not save the new settings to NAND.") => "No se pudo guardar la configuración en NAND.",
            (French, "Could not save the new settings to NAND.") => "Impossible d’enregistrer les nouveaux paramètres dans la NAND.",
            (German, "Could not save the new settings to NAND.") => "Die neuen Einstellungen konnten nicht im NAND gespeichert werden.",
            (Dutch, "Could not save the new settings to NAND.") => "De nieuwe instellingen kunnen niet op NAND worden opgeslagen.",
            (Italian, "Could not save the new settings to NAND.") => "Impossibile salvare le nuove impostazioni nella NAND.",
            (Japanese, "Could not save the new settings to NAND.") => "新しい設定をNANDに保存できません。",
            (Spanish, "Could not save settings. Application launch stopped.") => "No se pudo guardar la configuración. Se canceló el inicio de la aplicación.",
            (French, "Could not save settings. Application launch stopped.") => "Impossible d’enregistrer les paramètres. Le lancement de l’application est annulé.",
            (German, "Could not save settings. Application launch stopped.") => "Die Einstellungen konnten nicht gespeichert werden. Die Anwendung wird nicht gestartet.",
            (Dutch, "Could not save settings. Application launch stopped.") => "Instellingen kunnen niet worden opgeslagen. De applicatie wordt niet gestart.",
            (Italian, "Could not save settings. Application launch stopped.") => "Impossibile salvare le impostazioni. Avvio dell’applicazione annullato.",
            (Japanese, "Could not save settings. Application launch stopped.") => "設定を保存できません。アプリの起動を中止しました。",
            (Spanish, "Could not stop storage. Application launch stopped.") => "No se pudo detener el almacenamiento. Se canceló el inicio de la aplicación.",
            (French, "Could not stop storage. Application launch stopped.") => "Impossible d’arrêter le stockage. Le lancement de l’application est annulé.",
            (German, "Could not stop storage. Application launch stopped.") => "Der Speicher konnte nicht gestoppt werden. Die Anwendung wird nicht gestartet.",
            (Dutch, "Could not stop storage. Application launch stopped.") => "Opslag kan niet worden gestopt. De applicatie wordt niet gestart.",
            (Italian, "Could not stop storage. Application launch stopped.") => "Impossibile arrestare il dispositivo. Avvio dell’applicazione annullato.",
            (Japanese, "Could not stop storage. Application launch stopped.") => "ストレージを停止できません。アプリの起動を中止しました。",
            (Spanish, "Extract the received ZIP file?") => "¿Extraer el archivo ZIP recibido?",
            (French, "Extract the received ZIP file?") => "Extraire le fichier ZIP reçu ?",
            (German, "Extract the received ZIP file?") => "Empfangene ZIP-Datei entpacken?",
            (Dutch, "Extract the received ZIP file?") => "Het ontvangen ZIP-bestand uitpakken?",
            (Italian, "Extract the received ZIP file?") => "Estrarre il file ZIP ricevuto?",
            (Japanese, "Extract the received ZIP file?") => "受信したZIPファイルを展開しますか？",
            (Spanish, "of free space are required.") => "de espacio libre son necesarios.",
            (French, "of free space are required.") => "d'espace libre sont nécessaires.",
            (German, "of free space are required.") => "freier Speicherplatz wird benötigt.",
            (Dutch, "of free space are required.") => "vrije ruimte is vereist.",
            (Italian, "of free space are required.") => "di spazio libero sono necessari.",
            (Japanese, "of free space are required.") => "の空き容量が必要です。",
            (Spanish, "WARNING: Files in '%s' will be overwritten") => {
                "AVISO: Se sobrescribirán los archivos de '%s'."
            }
            (French, "WARNING: Files in '%s' will be overwritten") => {
                "ATTENTION : les fichiers de '%s' seront remplacés."
            }
            (German, "WARNING: Files in '%s' will be overwritten") => {
                "WARNUNG: Dateien in '%s' werden überschrieben."
            }
            (Dutch, "WARNING: Files in '%s' will be overwritten") => {
                "WAARSCHUWING: Bestanden in '%s' worden overschreven."
            }
            (Italian, "WARNING: Files in '%s' will be overwritten") => {
                "ATTENZIONE: i file in '%s' verranno sovrascritti."
            }
            (Japanese, "WARNING: Files in '%s' will be overwritten") => {
                "警告: '%s' 内のファイルは上書きされます。"
            }
            _ => source,
        }
    }

    /// Build the received-app ZIP confirmation body with localized fixed text.
    pub fn received_zip_confirmation(
        self,
        directory: &str,
        required_bytes: u64,
        overwrite: bool,
    ) -> String {
        let size = if required_bytes / 1024 > 999 {
            let hundredths = required_bytes.saturating_mul(100) / (1024 * 1024);
            format!("{}.{:02} MB", hundredths / 100, hundredths % 100)
        } else {
            format!("{} KB", required_bytes / 1024)
        };
        let mut message = format!(
            "{}\n{} {}",
            self.text("Extract the received ZIP file?"),
            size,
            self.text("of free space are required.")
        );
        if overwrite {
            message.push_str("\n\n");
            message.push_str(
                &self
                    .text("WARNING: Files in '%s' will be overwritten")
                    .replace("%s", directory),
            );
        }
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_language_selection_matches_hbc_and_falls_back_to_english() {
        assert_eq!(Language::from_console(-1, None), Language::English);
        assert_eq!(Language::from_console(1, None), Language::English);
        assert_eq!(Language::from_console(3, None), Language::French);
        assert_eq!(Language::from_console(2, None), Language::German);
        assert_eq!(Language::from_console(4, None), Language::Spanish);
        assert_eq!(Language::from_console(5, None), Language::Italian);
        assert_eq!(Language::from_console(6, None), Language::Dutch);
        assert_eq!(Language::from_console(7, None), Language::English);
        assert_eq!(Language::from_console(0, None), Language::English);
        let ko_theme = ThemeMetadata {
            languages: vec!["ko".into()],
            ..ThemeMetadata::default()
        };
        assert_eq!(
            Language::from_console(0, Some(&ko_theme)),
            Language::English
        );
        let ja_theme = ThemeMetadata {
            languages: vec!["ja".into()],
            ..ThemeMetadata::default()
        };
        assert_eq!(
            Language::from_console(0, Some(&ja_theme)),
            Language::Japanese
        );
    }
    #[test]
    fn translates_catalog_labels_and_preserves_unknown_strings() {
        assert_eq!(Language::Spanish.text("Back"), "Atrás");
        assert_eq!(Language::Spanish.text("Warning"), "Advertencia");
        assert_eq!(Language::French.text("Theme:"), "Thème :");
        assert_eq!(Language::Dutch.text("Options"), "Opties");
        assert_eq!(Language::French.text("Device:"), "Dispositif:");
        assert_eq!(
            Language::German.text("Sort applications by:"),
            "Anwendungen sortieren nach:"
        );
        assert_eq!(Language::Japanese.text("Yes"), "はい");
        assert_eq!(Language::Italian.text("Please wait..."), "Attendere...");
        assert_eq!(
            Language::Spanish.text("Could not delete this application."),
            "No se pudo eliminar esta aplicación."
        );
        assert_eq!(Language::German.text("Name"), "Name");
        let dynamic_message = String::from("unmapped message");
        assert_eq!(Language::French.text(&dynamic_message), dynamic_message);
    }

    #[test]
    fn received_zip_confirmation_includes_space_and_overwrite_context() {
        let plain = Language::English.received_zip_confirmation("demo", 2048, false);
        assert!(plain.contains("2 KB of free space"));
        assert!(!plain.contains("overwritten"));
        let overwrite = Language::German.received_zip_confirmation("demo", 1024 * 1024, true);
        assert!(overwrite.contains("1.00 MB"));
        assert!(overwrite.contains("'demo'"));
        assert!(overwrite.contains("überschrieben"));
    }
}
