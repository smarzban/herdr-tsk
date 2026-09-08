import assert from 'node:assert/strict';
import test from 'node:test';
import {wrapText} from '../public/board-wrap.js';
test('wrapping retains all text, aligns soft breaks, and hard-breaks oversized words',()=>{
 assert.deepEqual(wrapText('one two three',8),['one two ','three']);
 assert.deepEqual(wrapText('ABCDEFGHI',4),['ABCD','EFGH','I']);
 assert.deepEqual(wrapText('a\r\nb\rc\n',4),['a','b','c','']);
 assert.deepEqual(wrapText('界界界',4),['界界','界']);
 assert.deepEqual(wrapText('e\u0301e\u0301',1),['e\u0301','e\u0301']);
});
