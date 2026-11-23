/* Read-only C runtime probe for HBC Reborn release-date parsing. */
#include <gccore.h>
#include <ogc/system.h>
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
                           "20250101-1-1-1", "19691201000000", "-0010101", "2025010112"};
    for (unsigned i = 0; i < sizeof cases / sizeof cases[0]; ++i) check(cases[i]);
    SYS_Report("HBC_DATE done cases=%u\n", (unsigned)(sizeof cases / sizeof cases[0]));
    for (;;) VIDEO_WaitVSync();
}
