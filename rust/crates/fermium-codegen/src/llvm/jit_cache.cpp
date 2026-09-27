// The compile cache's hook into MCJIT (DECISIONS D317): an llvm::ObjectCache that hands MCJIT a machine-code
// object saved by an earlier run (so it neither optimizes nor generates code again), or keeps the object MCJIT
// just generated so the caller can save it. LLVM's C API has no ObjectCache, hence this C++ entry point.
#include "llvm/ExecutionEngine/ExecutionEngine.h"
#include "llvm/ExecutionEngine/ObjectCache.h"
#include "llvm/Support/MemoryBuffer.h"

#include <cstring>
#include <string>

namespace {
class FmObjectCache : public llvm::ObjectCache {
public:
  std::string hit;
  std::string saved;
  void notifyObjectCompiled(const llvm::Module *, llvm::MemoryBufferRef obj) override {
    saved.assign(obj.getBufferStart(), obj.getBufferSize());
  }
  std::unique_ptr<llvm::MemoryBuffer> getObject(const llvm::Module *) override {
    if (hit.empty())
      return nullptr;
    return llvm::MemoryBuffer::getMemBufferCopy(hit);
  }
};
} // namespace

// A cache that returns `obj` (len bytes; none when len is 0) for the next module MCJIT generates code for.
extern "C" void *fermium_jit_cache_new(const char *obj, size_t len) {
  auto *c = new FmObjectCache();
  if (len > 0)
    c->hit.assign(obj, len);
  return c;
}

// Use the cache for the engine's code generation (before the first function address is asked for).
extern "C" void fermium_jit_cache_attach(LLVMExecutionEngineRef ee, void *c) {
  llvm::unwrap(ee)->setObjectCache(static_cast<FmObjectCache *>(c));
}

// The object MCJIT generated (0 bytes when it used the cached one).
extern "C" size_t fermium_jit_cache_saved(void *c, const char **data) {
  auto *cc = static_cast<FmObjectCache *>(c);
  *data = cc->saved.data();
  return cc->saved.size();
}

// Free the cache (after the engine is gone).
extern "C" void fermium_jit_cache_free(void *c) { delete static_cast<FmObjectCache *>(c); }
