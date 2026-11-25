/* Minimal Wii app that verifies Reborn's HBCN shared-memory handoff. */
#include <stdio.h>

#include "hbc_netlog.h"

int main(void) {
	if (hbc_netlog_init() < 0)
		return 1;
	printf("HBCN_HANDOFF_OK\n");
	return 0;
}
