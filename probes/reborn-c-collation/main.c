/* Read-only probe for the locale and byte ordering used by Reborn's catalog. */
#include <gccore.h>
#include <locale.h>
#include <ogc/system.h>
#include <strings.h>

int main(void) {
    const char *locale = setlocale(LC_CTYPE, "C-ISO-8859-1");
    const int utf8_comparison = strcasecmp("\xc4\x80", "\xe0\xa0\x80");
    const int ascii_comparison = strcasecmp("Alpha", "alpha");
    SYS_Report("HBC_COLLATE locale=%s utf8_cmp=%d ascii_cmp=%d\n",
               locale ? locale : "NULL", utf8_comparison, ascii_comparison);
    if (locale && utf8_comparison < 0 && ascii_comparison == 0) {
        SYS_Report("PASS HBC_COLLATE ASCII-only UTF-8 byte order\n");
    } else {
        SYS_Report("FAIL HBC_COLLATE locale or comparison\n");
    }
    for (;;) {
        VIDEO_WaitVSync();
    }
}
