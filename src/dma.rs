//! DMA1 split into per-stream ownership tokens.
//!
//! The PAC hands out DMA1 as one singleton for the whole controller. `split` consumes it and
//! returns one zero-sized token per stream, so a driver owns exactly the stream it uses and the
//! compiler stops anything else from taking it.
//!
//! Sharing the controller is sound because each stream has its own CR/NDTR/PAR/M0AR/FCR, and the
//! shared flag registers are either read-only (LISR/HISR) or write-1-to-clear (LIFCR/HIFCR), so a
//! token touching only its own bits can't disturb another stream.

use stm32f4::stm32f411 as pac;

/// Ownership token for one DMA1 stream. Zero-sized; only `split` can create it,
/// so each stream exists exactly once.
pub struct Dma1Stream<const N: usize> {
    _private: (),
}

#[allow(dead_code)] // streams without a user yet
pub struct Dma1Streams {
    pub s0: Dma1Stream<0>,
    pub s1: Dma1Stream<1>,
    pub s2: Dma1Stream<2>,
    pub s3: Dma1Stream<3>,
    pub s4: Dma1Stream<4>,
    pub s5: Dma1Stream<5>,
    pub s6: Dma1Stream<6>,
    pub s7: Dma1Stream<7>,
}

/// Consumes the DMA1 singleton: after this nobody can reach "all of DMA1" anymore.
pub fn split(_dma: pac::DMA1) -> Dma1Streams {
    Dma1Streams {
        s0: Dma1Stream { _private: () },
        s1: Dma1Stream { _private: () },
        s2: Dma1Stream { _private: () },
        s3: Dma1Stream { _private: () },
        s4: Dma1Stream { _private: () },
        s5: Dma1Stream { _private: () },
        s6: Dma1Stream { _private: () },
        s7: Dma1Stream { _private: () },
    }
}

// Per-stream flags in LISR/HISR and LIFCR/HIFCR (RM0383 9.5.1-9.5.4): streams 0-3 live in the
// L registers, 4-7 in the H registers, each at bit offset 0, 6, 16 or 22. Within that group:
const FEIF: u32 = 1 << 0; // FIFO error
const DMEIF: u32 = 1 << 2; // direct mode error
const TEIF: u32 = 1 << 3; // transfer error
const HTIF: u32 = 1 << 4; // half transfer
const TCIF: u32 = 1 << 5; // transfer complete
const ALL_FLAGS: u32 = FEIF | DMEIF | TEIF | HTIF | TCIF;
const FLAG_OFFSET: [u32; 4] = [0, 6, 16, 22];

fn dma1() -> &'static pac::dma1::RegisterBlock {
    unsafe { &*pac::DMA1::ptr() }
}

/// Snapshot of one stream's interrupt flags.
#[derive(Clone, Copy)]
pub struct Flags(u32);

impl Flags {
    pub fn transfer_complete(self) -> bool {
        self.0 & TCIF != 0
    }
    pub fn transfer_error(self) -> bool {
        self.0 & TEIF != 0
    }
}

impl<const N: usize> Dma1Stream<N> {
    const SHIFT: u32 = FLAG_OFFSET[N % 4];

    /// Stream N's own registers (CR, NDTR, PAR, M0AR, FCR). Only the token owner can get them.
    pub fn regs(&self) -> &pac::dma1::ST {
        dma1().st(N)
    }

    /// Stream N's interrupt flags. Only the token owner reads them; the stream's interrupt
    /// handler is a doorbell that doesn't touch DMA registers at all.
    pub fn flags(&self) -> Flags {
        let isr = if N < 4 {
            dma1().lisr().read().bits()
        } else {
            dma1().hisr().read().bits()
        };
        Flags((isr >> Self::SHIFT) & ALL_FLAGS)
    }

    /// Clears stream N's flags. Required before the stream can be enabled again.
    /// Write-1-to-clear: the zeros written for other streams' bits leave them untouched.
    pub fn clear_flags(&self) {
        let bits = ALL_FLAGS << Self::SHIFT;
        if N < 4 {
            dma1().lifcr().write(|w| unsafe { w.bits(bits) });
        } else {
            dma1().hifcr().write(|w| unsafe { w.bits(bits) });
        }
    }
}
