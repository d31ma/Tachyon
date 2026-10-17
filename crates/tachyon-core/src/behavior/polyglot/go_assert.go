package main

import (
	"fmt"
	"reflect"
)

type BehaviorAssert struct{}

var Assert BehaviorAssert

type BehaviorAssertionFailure struct {
	Message string
}

const behaviorAssertNothingThrown = "Expected an error to be thrown, but nothing was thrown."
const behaviorAssertSeparator = " ==> "

func (failure BehaviorAssertionFailure) Error() string {
	return failure.Message
}

func (BehaviorAssert) Equals(expected any, actual any, message ...string) {
	failure := fmt.Sprintf("expected: <%v> but was: <%v>", expected, actual)
	behaviorAssertCheck(behaviorAssertSame(expected, actual), failure, message)
}

func (BehaviorAssert) IsTrue(condition bool, message ...string) {
	failure := fmt.Sprintf("expected: <true> but was: <%v>", condition)
	behaviorAssertCheck(condition, failure, message)
}

func (BehaviorAssert) Throws(action func() error, message ...string) error {
	thrown := behaviorAssertThrown(action)
	behaviorAssertCheck(thrown != nil, behaviorAssertNothingThrown, message)
	return thrown
}

func behaviorAssertThrown(action func() error) (thrown error) {
	defer func() {
		if recovered := recover(); recovered != nil {
			thrown = behaviorAssertRecovered(recovered)
		}
	}()
	return action()
}

func behaviorClearFake[T any](field *T) {
	var empty T
	*field = empty
}

func behaviorAssertRecovered(recovered any) error {
	if err, isError := recovered.(error); isError {
		return err
	}
	return fmt.Errorf("%v", recovered)
}

func behaviorAssertSame(expected any, actual any) bool {
	expectedNumber, expectedIsNumber := behaviorAssertNumber(expected)
	actualNumber, actualIsNumber := behaviorAssertNumber(actual)
	if expectedIsNumber && actualIsNumber {
		return expectedNumber == actualNumber
	}
	return reflect.DeepEqual(expected, actual)
}

func behaviorAssertNumber(value any) (float64, bool) {
	reflected := reflect.ValueOf(value)
	switch reflected.Kind() {
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return float64(reflected.Int()), true
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64:
		return float64(reflected.Uint()), true
	case reflect.Float32, reflect.Float64:
		return reflected.Float(), true
	}
	return 0, false
}

func behaviorAssertCheck(passed bool, failure string, message []string) {
	if !passed {
		panic(BehaviorAssertionFailure{Message: behaviorAssertText(failure, message)})
	}
}

func behaviorAssertText(failure string, message []string) string {
	if len(message) == 0 {
		return failure
	}
	return message[0] + behaviorAssertSeparator + failure
}
