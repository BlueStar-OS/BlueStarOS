use crate::driver::network::e1000::{
    agreenment::{
        DestIp, EthHead, EtherType, IPv4Header, Ipv4Addr, Ipv4Protocol, SourceIp, SourceMac,
    },
    arp::ARP_TABLE,
    netbuffer::NetBuffer,
    tx_ringbuffer::{e1000_transmit, E1000TxRing},
    E1000_DEV,
};

/// 本机 IPv4 地址 (临时硬编码，后续应从网络配置读取)
#[allow(dead_code)]
const MY_IPV4: Ipv4Addr = Ipv4Addr::new([10, 0, 0, 2]);

/// 发送 IPv4 报文
#[allow(dead_code)]
///
/// 1. 查 ARP 表获得目标 MAC
/// 2. 构造 IPv4 首部 (含校验和)
/// 3. 依次压入 IP 头、以太网头
/// 4. 通过 e1000 发送
pub fn send_ipv4_packet(target_ip: [u8; 4], protocol: Ipv4Protocol, payload: NetBuffer) {
    let mut payload = payload;
    let target_ip = Ipv4Addr::new(target_ip);

    // 1. 查 ARP 表获得目标 MAC
    // 挂起队列 + ARP Request 主动解析
    let target_mac = ARP_TABLE
        .lock()
        .lookup(&target_ip)
        .copied()
        .expect("send_ipv4_packet: ARP cache miss — send ARP request first");

    // 2. 获取本机 MAC (Copy 类型，短时加锁后即释放)
    let local_mac = {
        let guard = E1000_DEV.lock();
        guard.as_ref().expect("no e1000 device").mac
    };

    // 3. 构造 IPv4 头
    let ip_payload_len = payload.data_len() as u16;
    let mut ip_hdr = IPv4Header::new(
        protocol,
        ip_payload_len,
        SourceIp::new(MY_IPV4.octets()),
        DestIp::new(target_ip.octets()),
    );
    ip_hdr.checksum_rfc_for_self(); // RFC 1071 校验和

    // 4. 压入 IP 头 (push)
    payload.new_agreement_head(&ip_hdr);

    // 5. 构造以太网头
    let eth_hdr = EthHead::new(target_mac, SourceMac(local_mac), EtherType::IPV4);

    // 6. 压入以太网头 (push)
    payload.new_agreement_head(&eth_hdr);

    // 7. 发送
    let mut guard = E1000_DEV.lock();
    let dev = (*guard).as_mut().expect("no e1000 device");
    let tx_ring = unsafe { &mut *((&mut dev.tx_ring) as *mut E1000TxRing) };
    e1000_transmit(&dev.bar, tx_ring, payload);
}
