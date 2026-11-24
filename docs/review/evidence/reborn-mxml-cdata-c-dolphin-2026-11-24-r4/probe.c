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

static void report_case(unsigned index, const char *xml) {
    mxml_node_t *root = mxmlLoadString(NULL, xml, MXML_OPAQUE_CALLBACK);
    mxml_node_t *app = root ? mxmlFindElement(root, root, "app", NULL, NULL,
                                                MXML_DESCEND_FIRST) : NULL;
    mxml_node_t *name = app ? mxmlFindElement(app, app, "name", NULL, NULL,
                                               MXML_DESCEND_FIRST) : NULL;
    const char *text = first_opaque(name);
    SYS_Report("HBC_CDATA index=%u value=%s\n", index, text ? text : "<none>");
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
    report_case(0, "<root><app><name>first<b>nested</b>last</name></app></root>");
    report_case(1, "<root><app><name>first<!--gap-->last</name></app></root>");
    report_case(2, "<root><app><name><b>nested</b></name></app></root>");
    report_case(3, "<root><app><name><![CDATA[cdata]]>last</name></app></root>");
    report_case(4, "<root><app><name><![CDATA[cdata]]></name></app></root>");
    SYS_Report("HBC_CDATA done cases=5\n");
    for (;;) VIDEO_WaitVSync();
}
