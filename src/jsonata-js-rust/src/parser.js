/**
 * Hybrid parser that prefers the Rust implementation and falls back to the
 * original JavaScript Pratt parser when the native addon is unavailable.
 */

(function () {
    'use strict';

    const upstreamParser = require('../../jsonata/src/parser');
    const native = require('../native/index.node');

    function reviveRegexLiterals(node) {
        if (!node || typeof node !== 'object') {
            return node;
        }

        if (Array.isArray(node)) {
            node.forEach(reviveRegexLiterals);
            return node;
        }

        if (node.type === 'regex' && node.value && typeof node.value.pattern === 'string') {
            var flags = node.value.flags || 'g';
            try {
                node.value = new RegExp(node.value.pattern, flags);
            } catch (err) {
                throw ensureStack(err);
            }
        }

        Object.keys(node).forEach(function (key) {
            if (key === 'value' && node.type === 'regex') {
                return;
            }
            reviveRegexLiterals(node[key]);
        });

        return node;
    }

    function callRustParser(source, recover) {
        if (!native || typeof native.parseExpression !== 'function') {
            return null;
        }

        const result = native.parseExpression(source, recover);
        if (result && typeof result === 'object') {
            if (result.ok === true && result.ast) {
                reviveRegexLiterals(result.ast);
            }
            return result;
        }

        return null;
    }

    function ensureStack(error) {
        if (error && typeof error === 'object' && !error.stack) {
            error.stack = (new Error()).stack;
        }
        return error;
    }

    function parser(source, recover) {
        const rustResult = callRustParser(source, Boolean(recover));
        if (rustResult) {
            if (rustResult.ok === true) {
                return rustResult.ast;
            }
            if (rustResult.error) {
                throw ensureStack(rustResult.error);
            }
        }

        return upstreamParser(source, recover);
    }

    module.exports = parser;
}());
