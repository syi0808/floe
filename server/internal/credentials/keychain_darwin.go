//go:build darwin && cgo && !floe_dev

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
    CFDictionarySetValue(query, kSecAttrSynchronizable, kCFBooleanFalse);
    CFDictionarySetValue(query, kSecUseAuthenticationUI, kSecUseAuthenticationUIFail);
    OSStatus status;
    if (operation == 0) {
        CFDictionarySetValue(query, kSecReturnData, kCFBooleanTrue);
        CFTypeRef data = NULL;
        status = SecItemCopyMatching(query, &data);
        if (status == errSecItemNotFound) status = errSecSuccess;
        else if (status == errSecSuccess && data == NULL) status = errSecDecode;
        if (status == errSecSuccess && data != NULL) {
            if (CFGetTypeID(data) != CFDataGetTypeID() || CFDataGetLength((CFDataRef)data) == 0 || CFDataGetLength((CFDataRef)data) > 131072) {
                CFRelease(data); CFRelease(query); CFRelease(account); return errSecDecode;
            }
            CFIndex length = CFDataGetLength((CFDataRef)data);
            if (memchr(CFDataGetBytePtr((CFDataRef)data), 0, length) != NULL) {
                CFRelease(data); CFRelease(query); CFRelease(account); return errSecDecode;
            }
            *output = malloc(length + 1);
            if (*output == NULL) {CFRelease(data); CFRelease(query); CFRelease(account); return errSecAllocate;}
            memcpy(*output, CFDataGetBytePtr((CFDataRef)data), length);
            (*output)[length] = 0;
            CFRelease(data);
        }
    } else if (operation == 1) {
        CFDataRef data = CFDataCreate(NULL, (const UInt8 *)value, strlen(value));
        CFDictionarySetValue(query, kSecValueData, data);
        CFDictionarySetValue(query, kSecAttrAccessible, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly);
        status = SecItemAdd(query, NULL);
		if (status == errSecDuplicateItem) {
			CFDictionaryRemoveValue(query, kSecValueData);
			CFDictionaryRemoveValue(query, kSecAttrAccessible);
			CFMutableDictionaryRef update = CFDictionaryCreateMutable(NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
			CFDictionarySetValue(update, kSecValueData, data);
            CFDictionarySetValue(update, kSecAttrAccessible, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly);
			status = SecItemUpdate(query, update);
			CFRelease(update);
		}
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
	"unsafe"
)

func nativeKeychain(name, value string, operation int) (string, error) {
	account, secret := C.CString(name), C.CString(value)
	defer C.free(unsafe.Pointer(account))
	defer C.free(unsafe.Pointer(secret))
	var output *C.char
	status := C.floe_keychain(account, secret, C.int(operation), &output)
	if status == C.errSecInteractionNotAllowed || status == C.errSecAuthFailed {
		return "", ErrLocked
	}
	if status != 0 {
		return "", ErrUnavailable
	}
	if output == nil {
		return "", nil
	}
	defer C.free(unsafe.Pointer(output))
	return C.GoString(output), nil
}
