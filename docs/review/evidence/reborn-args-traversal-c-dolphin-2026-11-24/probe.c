#include <gccore.h>
#include <ogc/system.h>
#include <mxml.h>
#include <stdio.h>

static const char *first_opaque(mxml_node_t *node) {
    mxml_node_t *child = mxmlGetFirstChild(node);
    while (child) {
        if (mxmlGetType(child) == MXML_OPAQUE)
            return mxmlGetOpaque(child);
        child = mxmlWalkNext(child, node, MXML_NO_DESCEND);
    }
    return NULL;
}

static void report_args(unsigned index, const char *xml) {
    mxml_node_t *root = mxmlLoadString(NULL, xml, MXML_OPAQUE_CALLBACK);
    mxml_node_t *args = root ? mxmlFindElement(root, root, "arguments", NULL, NULL,
                                                MXML_DESCEND_FIRST) : NULL;
    unsigned count = 0;
    mxml_node_t *arg = args ? mxmlFindElement(args, args, "arg", NULL, NULL,
                                               MXML_DESCEND_FIRST) : NULL;
    while (arg) {
        SYS_Report("HBC_ARGS index=%u item=%u value=%s\n", index, count,
                   first_opaque(arg) ? first_opaque(arg) : "<none>");
        ++count;
        arg = mxmlFindElement(arg, args, "arg", NULL, NULL, MXML_NO_DESCEND);
    }
    SYS_Report("HBC_ARGS index=%u count=%u\n", index, count);
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
    report_args(0, "<root><arguments><arg>direct-a</arg><arg>direct-b</arg></arguments></root>");
    report_args(1, "<root><arguments><wrap><arg>nested</arg></wrap><arg>direct</arg></arguments></root>");
    report_args(2, "<root><arguments><arg>direct-a</arg><wrap><arg>nested</arg></wrap><arg>direct-b</arg></arguments></root>");
    SYS_Report("HBC_ARGS done cases=3\n");
    for (;;) VIDEO_WaitVSync();
}
