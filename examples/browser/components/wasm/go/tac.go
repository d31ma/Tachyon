package main

import "unsafe"

var input [2048]byte
var output [128]byte
var outputLen uint32
var clicks uint32

func writeState() {
	pos := 0
	pos = copyText(pos, `{"state":{"clicks":`)
	pos = writeUint(pos, clicks)
	pos = copyText(pos, `,"label":"Go"}}`)
	outputLen = uint32(pos)
}

func copyText(pos int, text string) int {
	for i := 0; i < len(text); i++ {
		output[pos] = text[i]
		pos++
	}
	return pos
}

func writeUint(pos int, value uint32) int {
	var digits [10]byte
	cursor := len(digits)
	for {
		cursor--
		digits[cursor] = byte('0' + value%10)
		value /= 10
		if value == 0 {
			break
		}
	}
	for cursor < len(digits) {
		output[pos] = digits[cursor]
		pos++
		cursor++
	}
	return pos
}

func readClicks(payload []byte) uint32 {
	marker := []byte(`"clicks":`)
	for i := 0; i+len(marker) <= len(payload); i++ {
		matched := true
		for j := 0; j < len(marker); j++ {
			if payload[i+j] != marker[j] {
				matched = false
				break
			}
		}
		if matched {
			cursor := i + len(marker)
			var value uint32
			for cursor < len(payload) && payload[cursor] >= '0' && payload[cursor] <= '9' {
				value = value*10 + uint32(payload[cursor]-'0')
				cursor++
			}
			return value
		}
	}
	return 0
}

//export alloc
func alloc(size uint32) uint32 {
	if size > uint32(len(input)) {
		return 0
	}
	return uint32(uintptr(unsafe.Pointer(&input[0])))
}

//export dealloc
func dealloc(ptr uint32, length uint32) {}

//export init
func initTac(ptr uint32, length uint32) {
	clicks = 0
	writeState()
}

//export call
func call(methodPtr uint32, methodLen uint32, payloadPtr uint32, payloadLen uint32) {
	clicks = readClicks(input[:payloadLen]) + 1
	writeState()
}

//export output_ptr
func output_ptr() uint32 {
	return uint32(uintptr(unsafe.Pointer(&output[0])))
}

//export output_len
func output_len() uint32 {
	return outputLen
}

func main() {}
