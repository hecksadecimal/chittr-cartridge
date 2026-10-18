/* Structural WAD rejection tests; each case runs in a fresh native process. */
#undef printf
#undef fprintf
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include "doom.h"

static void put32(unsigned char *p,uint32_t value)
{
    p[0]=(unsigned char)value; p[1]=(unsigned char)(value>>8);
    p[2]=(unsigned char)(value>>16); p[3]=(unsigned char)(value>>24);
}
int test_invalid_wad(const char *which)
{
    unsigned char data[64] = {0};
    const unsigned char *input = data;
    const char *expected = "";
    int len = sizeof(data);
    memcpy(data,"PWAD",4);
    put32(data+4,1); put32(data+8,12);
    memcpy(data+20,"TITLEPIC",8);
    if (!strcmp(which,"skill"))
    {
        if (doom_boot(data,(int)sizeof(data),5) != -1 || !strstr(doom_error(),"skill") || doom_pixels())
        {
            fprintf(stderr,"FAIL reject skill: %s\n",doom_error());
            return 1;
        }
        printf("PASS reject skill: %s\n",doom_error());
        return 0;
    }
    if (!strcmp(which,"null")) { input=NULL; expected="truncated header"; }
    else if (!strcmp(which,"negative")) { len=-1; expected="truncated header"; }
    else if (!strcmp(which,"short")) { len=11; expected="truncated header"; }
    else if (!strcmp(which,"magic")) { data[0]='X'; expected="IWAD or PWAD"; }
    else if (!strcmp(which,"empty")) { put32(data+4,0); expected="directory outside"; }
    else if (!strcmp(which,"count")) { put32(data+4,0xffffffffu); expected="directory outside"; }
    else if (!strcmp(which,"directory")) { put32(data+8,60); expected="directory outside"; }
    else if (!strcmp(which,"dirwrap")) { put32(data+8,0xffffffffu); expected="directory outside"; }
    else if (!strcmp(which,"position")) { put32(data+12,65); expected="lump 0 outside"; }
    else if (!strcmp(which,"size")) { put32(data+12,60); put32(data+16,5); expected="lump 0 outside"; }
    else if (!strcmp(which,"sizewrap")) { put32(data+16,0xffffffffu); expected="lump 0 outside"; }
    else if (!strcmp(which,"incomplete")) { expected="incomplete game data"; }
    else { fprintf(stderr,"unknown invalid test %s\n",which); return 2; }
    if (doom_boot(input,len,2) != -1 || !strstr(doom_error(),expected) || doom_pixels())
    {
        fprintf(stderr,"FAIL reject %s: %s\n",which,doom_error());
        return 1;
    }
    doom_tick(); /* error state is inert, not an exit/trap */
    doom_key(0,1);
    if (doom_boot(input,len,2) != -1)
    {
        fprintf(stderr,"FAIL: second boot allowed\n");
        return 1;
    }
    printf("PASS reject %s: %s\n",which,doom_error());
    return 0;
}
