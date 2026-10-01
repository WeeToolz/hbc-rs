//! HBC's system-language selection and UI string catalog.
use crate::metadata::ThemeMetadata;

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

    pub fn text(self, source: &'static str) -> &'static str {
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
            _ => source,
        }
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
        assert_eq!(Language::Dutch.text("Options"), "Opties");
        assert_eq!(Language::French.text("Device:"), "Dispositif:");
        assert_eq!(
            Language::German.text("Sort applications by:"),
            "Anwendungen sortieren nach:"
        );
        assert_eq!(Language::Japanese.text("Yes"), "はい");
        assert_eq!(Language::German.text("Name"), "Name");
        assert_eq!(Language::French.text("not in catalog"), "not in catalog");
    }
}
