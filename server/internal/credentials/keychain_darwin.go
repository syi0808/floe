package credentials

/*
#cgo LDFLAGS: -framework Security -framework CoreFoundation
#include <Security/Security.h>
#include <stdlib.h>
#include <string.h>

static int floe_keychain(const char *name, const char *value, int operation, char **output) {
    CFStringRef account = CFStringCreateWithCString(NULL, name, kCFStringEncodingUTF8);
    CFMutableDictionaryRef query = CFDictionaryCreateMutable(NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    CFDictionarySetValue(query, kSecClass, kSecClassGenericPassword);
    CFDictionarySetValue(query, kSecAttrService, CFSTR("app.floe.server.credentials"));
    CFDictionarySetValue(query, kSecAttrAccount, account);
    OSStatus status;
    if (operation == 0) {
        CFDictionarySetValue(query, kSecReturnData, kCFBooleanTrue);
        CFTypeRef data = NULL;
        status = SecItemCopyMatching(query, &data);
        if (status == errSecSuccess) {
            CFIndex length = CFDataGetLength((CFDataRef)data);
            *output = malloc(length + 1);
            memcpy(*output, CFDataGetBytePtr((CFDataRef)data), length);
            (*output)[length] = 0;
            CFRelease(data);
        }
    } else if (operation == 1) {
        CFDataRef data = CFDataCreate(NULL, (const UInt8 *)value, strlen(value));
        CFDictionarySetValue(query, kSecValueData, data);
        CFDictionarySetValue(query, kSecAttrAccessible, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly);
        status = SecItemAdd(query, NULL);
        CFRelease(data);
    } else {
        status = SecItemDelete(query);
        if (status == errSecItemNotFound) status = errSecSuccess;
    }
    CFRelease(query);
    CFRelease(account);
    return status;
}
*/
import "C"

import (
	"errors"
	"strings"
	"unsafe"
)

type Keychain struct{}

func (Keychain) Get(name string) (string, error) { return keychain(name, "", 0) }
func (Keychain) Put(name, value string) error    { _, err := keychain(name, value, 1); return err }
func (Keychain) Delete(name string) error        { _, err := keychain(name, "", 2); return err }

func keychain(name, value string, operation int) (string, error) {
	if strings.ContainsRune(name+value, 0) {
		return "", errors.New("invalid credential")
	}
	account, secret := C.CString(name), C.CString(value)
	defer C.free(unsafe.Pointer(account))
	defer C.free(unsafe.Pointer(secret))
	var output *C.char
	if C.floe_keychain(account, secret, C.int(operation), &output) != 0 {
		return "", errors.New("credential store unavailable")
	}
	if output == nil {
		return "", nil
	}
	defer C.free(unsafe.Pointer(output))
	return C.GoString(output), nil
}
