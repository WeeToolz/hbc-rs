/* Read-only C runtime probe for HBC Reborn release-date parsing. */
#include <gccore.h>
#include <ogc/system.h>
#include <mxml.h>
#include <stdio.h>
#include <string.h>
#include <time.h>

static void check(const char *s) {
    struct tm t;
    memset(&t, 0, sizeof t);
    int count = sscanf(s, "%4d%2d%2d%2d%2d%2d", &t.tm_year, &t.tm_mon,
                       &t.tm_mday, &t.tm_hour, &t.tm_min, &t.tm_sec);
    if (count != 6) {
        count = sscanf(s, "%4d%2d%2d", &t.tm_year, &t.tm_mon, &t.tm_mday);
        if (count != 3) {
            SYS_Report("HBC_DATE input=%s count=%d invalid\n", s, count);
            return;
        }
        t.tm_hour = 1;
        t.tm_min = 0;
        t.tm_sec = 0;
    }
    t.tm_year -= 1900;
    time_t value = mktime(&t);
    SYS_Report("HBC_DATE input=%s count=%d epoch=%lld normalized=%d,%d,%d,%d,%d,%d\n",
               s, count, (long long)value, t.tm_year + 1900, t.tm_mon + 1,
               t.tm_mday, t.tm_hour, t.tm_min, t.tm_sec);
}

static void check_update_date(unsigned index, const char *value, bool present) {
    char xml[128];
    if (present)
        snprintf(xml, sizeof xml, "<update><app><date>%s</date></app></update>", value);
    else
        snprintf(xml, sizeof xml, "<update><app></app></update>");

    mxml_node_t *root = mxmlLoadString(NULL, xml, MXML_OPAQUE_CALLBACK);
    mxml_node_t *app = root ? mxmlFindElement(root, root, "app", NULL, NULL,
                                               MXML_DESCEND_FIRST) : NULL;
    mxml_node_t *date = app ? mxmlFindElement(app, app, "date", NULL, NULL,
                                              MXML_DESCEND_FIRST) : NULL;
    mxml_node_t *child = date ? mxmlGetFirstChild(date) : NULL;
    const char *text = child ? mxmlGetOpaque(child) : NULL;
    unsigned long long parsed = 0;
    if (text && sscanf(text, "%llu", &parsed) != 1)
        parsed = 0;
    SYS_Report("HBC_UPDATE index=%u value=%llu\n", index, parsed);
    if (root)
        mxmlDelete(root);
}

int main(void) {
    VIDEO_Init();
    GXRModeObj *mode = VIDEO_GetPreferredMode(NULL);
    void *framebuffer = MEM_K0_TO_K1(SYS_AllocateFramebuffer(mode));
    VIDEO_Configure(mode);
    VIDEO_ClearFrameBuffer(mode, framebuffer, COLOR_BLACK);
    VIDEO_SetNextFramebuffer(framebuffer);
    VIDEO_SetBlack(FALSE);
    VIDEO_Flush();
    VIDEO_WaitVSync();
    const char *cases[] = {"20241332", "20250000", "20250101999999", "2025-01-02",
                           "20250101-1-1-1", "19691201000000", "-0010101", "2025010112",
                           "2025 1 1", "2025x0101", "20250101123456tail", "202501011234",
                           "20240131", "20250131", "20241231", "20250131235960", "20250102"};
    for (unsigned i = 0; i < sizeof cases / sizeof cases[0]; ++i) check(cases[i]);
    const struct { const char *value; bool present; } updates[] = {
        {"123tail", true}, {"  +456rest", true}, {"-7tail", true},
        {"junk", true}, {"0", true}, {"", true}, {"\xc2\xa0" "123", true},
        {"", false},
    };
    for (unsigned i = 0; i < sizeof updates / sizeof updates[0]; ++i)
        check_update_date(i, updates[i].value, updates[i].present);
    SYS_Report("HBC_META done dates=%u updates=%u\n",
               (unsigned)(sizeof cases / sizeof cases[0]),
               (unsigned)(sizeof updates / sizeof updates[0]));
    for (;;) VIDEO_WaitVSync();
}
